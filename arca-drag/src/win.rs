//! The Windows half: an `IDataObject` that extracts on demand, hosted where the
//! shell's calls do not land on the window's thread, and the calls that start a
//! drag the way the Explorer starts its own.

use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

use windows::core::{implement, Interface, Result, HRESULT, PCWSTR};
use windows::Win32::Foundation::{
    GlobalFree, BOOL, DATA_S_SAMEFORMATETC, DV_E_FORMATETC, DV_E_TYMED, E_NOTIMPL, HANDLE, HGLOBAL,
    OLE_E_ADVISENOTSUPPORTED, S_OK,
};
use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
use windows::Win32::System::Com::Marshal::CoMarshalInterThreadInterfaceInStream;
use windows::Win32::System::Com::StructuredStorage::CoGetInterfaceAndReleaseStream;
use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, IAdviseSink, IBindCtx, IDataObject, IDataObject_Impl,
    IEnumFORMATETC, IEnumSTATDATA, IStream, COINIT_MULTITHREADED, DATADIR, DATADIR_GET, FORMATETC,
    STGMEDIUM, TYMED_HGLOBAL, TYMED_ISTREAM,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{
    IDropSource, OleDuplicateData, OleInitialize, OleUninitialize, ReleaseStgMedium, DROPEFFECT,
    DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE,
};
use windows::Win32::UI::Shell::{
    IDataObjectAsyncCapability, IDataObjectAsyncCapability_Impl, SHCreateStdEnumFmtEtc,
    SHCreateStreamOnFileEx, SHDoDragDrop, FD_ATTRIBUTES, FD_FILESIZE, FD_WRITESTIME,
    FILEDESCRIPTORW,
};

/// What one entry looks like to the shell before anything has been extracted.
pub struct Item {
    /// Where it lands relative to the drop, backslashes and all. The shell
    /// makes any folders the path names.
    pub name: String,
    pub size: u64,
    /// Seconds since the Unix epoch, when the archive recorded one.
    pub mtime: Option<i64>,
}

/// What the drop turned out to be.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Effect {
    None,
    Copy,
    Move,
}

/// Why an entry was not handed over. The shell treats a cancel as the user's
/// own choice and stops quietly; a failure it reports.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Refused {
    Cancelled,
    Failed,
}

fn effect_of(e: DROPEFFECT) -> Effect {
    if e.0 & DROPEFFECT_MOVE.0 != 0 {
        Effect::Move
    } else if e.0 & DROPEFFECT_COPY.0 != 0 {
        Effect::Copy
    } else {
        Effect::None
    }
}

fn format(name: &str) -> u16 {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    // Registering a name that is already registered returns the same number,
    // which is how every program agrees on what these mean.
    unsafe { RegisterClipboardFormatW(PCWSTR(wide.as_ptr())) as u16 }
}

struct Formats {
    descriptor: u16,
    contents: u16,
    preferred: u16,
}

impl Formats {
    fn get() -> Self {
        Formats {
            descriptor: format("FileGroupDescriptorW"),
            contents: format("FileContents"),
            preferred: format("Preferred DropEffect"),
        }
    }
}

/// Copies `bytes` into a moveable global block, which is how everything travels
/// through the clipboard and through a drag.
unsafe fn to_global(bytes: &[u8]) -> Result<HGLOBAL> {
    let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len())?;
    let at = GlobalLock(handle);
    if at.is_null() {
        return Err(windows::core::Error::from_win32());
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), at.cast::<u8>(), bytes.len());
    let _ = GlobalUnlock(handle);
    Ok(handle)
}

/// Unix seconds as a Windows FILETIME: hundreds of nanoseconds since 1601.
fn to_filetime(unix: i64) -> u64 {
    const EPOCH_DIFFERENCE: i64 = 11_644_473_600;
    ((unix + EPOCH_DIFFERENCE).max(0) as u64).saturating_mul(10_000_000)
}

/// What the shell asks the data object to keep: the drag image, and whatever
/// else the drop helpers need to pass between processes. Global handles, kept
/// as numbers so the object can be shared between threads.
struct Kept {
    format: u16,
    handle: isize,
}

type Deliver = Box<dyn Fn(usize) -> std::result::Result<PathBuf, Refused> + Send + Sync>;
type Ended = Box<dyn FnOnce(bool) + Send>;

/// Says once that the drop is over, and whether it went through. An async
/// drop ends when the shell calls `EndOperation`; a sync one when the drag
/// returns; and a drop the shell let go of without either, when the object
/// goes.
struct Ending {
    started: AtomicBool,
    call: Mutex<Option<Ended>>,
}

impl Ending {
    fn finish(&self, through: bool) {
        let call = self.call.lock().ok().and_then(|mut call| call.take());
        if let Some(call) = call {
            call(through);
        }
    }
}

#[implement(IDataObject, IDataObjectAsyncCapability)]
struct Source {
    items: Vec<Item>,
    deliver: Deliver,
    formats: Formats,
    kept: Mutex<Vec<Kept>>,
    asynchronous: AtomicBool,
    running: AtomicBool,
    ending: Arc<Ending>,
    /// Told when the last reference goes, so the thread keeping this object's
    /// apartment alive knows it can let go of it.
    gone: Mutex<Option<Sender<()>>>,
}

impl Drop for Source {
    fn drop(&mut self) {
        self.ending.finish(false);
        if let Ok(kept) = self.kept.get_mut() {
            for item in kept.drain(..) {
                unsafe {
                    let _ = GlobalFree(HGLOBAL(item.handle as *mut _));
                }
            }
        }
        if let Ok(gone) = self.gone.get_mut() {
            if let Some(tx) = gone.take() {
                let _ = tx.send(());
            }
        }
    }
}

impl Source {
    /// The block that says what is being dragged: a count, then one fixed-size
    /// record per item, in one allocation.
    unsafe fn descriptor(&self) -> Result<HGLOBAL> {
        let count = self.items.len();
        let head = std::mem::size_of::<u32>();
        let each = std::mem::size_of::<FILEDESCRIPTORW>();
        let mut bytes = vec![0u8; head + each * count];
        std::ptr::write_unaligned(bytes.as_mut_ptr().cast::<u32>(), count as u32);

        for (i, item) in self.items.iter().enumerate() {
            let mut fd = FILEDESCRIPTORW {
                dwFlags: (FD_ATTRIBUTES.0 | FD_FILESIZE.0) as u32,
                dwFileAttributes: FILE_ATTRIBUTE_NORMAL.0,
                nFileSizeHigh: (item.size >> 32) as u32,
                nFileSizeLow: (item.size & 0xFFFF_FFFF) as u32,
                ..Default::default()
            };
            if let Some(t) = item.mtime.filter(|t| *t > 0) {
                let ticks = to_filetime(t);
                fd.dwFlags |= FD_WRITESTIME.0 as u32;
                fd.ftLastWriteTime.dwHighDateTime = (ticks >> 32) as u32;
                fd.ftLastWriteTime.dwLowDateTime = (ticks & 0xFFFF_FFFF) as u32;
            }
            // cFileName is a fixed 260 wide characters with a terminator. A
            // name longer than that cannot be described here at all, so it is
            // cut rather than left to run over the end of the record.
            // Filled to one side and assigned whole: the record is packed, so
            // taking a reference to the field inside it would be unaligned.
            let mut name = [0u16; 260];
            let mut wide: Vec<u16> = Vec::with_capacity(259);
            for c in item.name.chars() {
                if wide.len() + c.len_utf16() > 259 {
                    break;
                }
                let mut unit = [0u16; 2];
                wide.extend_from_slice(c.encode_utf16(&mut unit));
            }
            name[..wide.len()].copy_from_slice(&wide);
            fd.cFileName = name;
            std::ptr::write_unaligned(
                bytes
                    .as_mut_ptr()
                    .add(head + each * i)
                    .cast::<FILEDESCRIPTORW>(),
                fd,
            );
        }
        to_global(&bytes)
    }

    fn keeps(&self, format: u16) -> bool {
        self.kept
            .lock()
            .is_ok_and(|kept| kept.iter().any(|k| k.format == format))
    }

    /// A copy of a block the shell parked here, made while the lock is held:
    /// the shell's threads call in at the same time, and a `SetData` for the
    /// same format frees the old block the moment it has the lock.
    fn kept_copy(&self, format: u16) -> Option<Result<HGLOBAL>> {
        let kept = self.kept.lock().ok()?;
        let handle = kept.iter().find(|k| k.format == format)?.handle;
        let copy = unsafe {
            OleDuplicateData(
                HANDLE(handle as *mut _),
                windows::Win32::System::Ole::CLIPBOARD_FORMAT(format),
                GMEM_MOVEABLE,
            )
        };
        Some(if copy.is_invalid() {
            Err(windows::core::Error::from_win32())
        } else {
            Ok(HGLOBAL(copy.0))
        })
    }
}

fn hglobal_medium(handle: HGLOBAL) -> STGMEDIUM {
    STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: windows::Win32::System::Com::STGMEDIUM_0 { hGlobal: handle },
        pUnkForRelease: std::mem::ManuallyDrop::new(None),
    }
}

impl IDataObject_Impl for Source_Impl {
    fn GetData(&self, request: *const FORMATETC) -> Result<STGMEDIUM> {
        let request = unsafe { *request };
        let wanted = request.cfFormat;

        if wanted == self.formats.descriptor && request.tymed & TYMED_HGLOBAL.0 as u32 != 0 {
            let block = unsafe { self.descriptor()? };
            return Ok(hglobal_medium(block));
        }

        if wanted == self.formats.contents && request.tymed & TYMED_ISTREAM.0 as u32 != 0 {
            // lindex says which one, and this is the moment it gets extracted.
            let which = request.lindex;
            if which < 0 || which as usize >= self.items.len() {
                return Err(DV_E_FORMATETC.into());
            }
            let path = match (self.deliver)(which as usize) {
                Ok(path) => path,
                Err(Refused::Cancelled) => {
                    return Err(windows::Win32::Foundation::ERROR_CANCELLED
                        .to_hresult()
                        .into())
                }
                Err(Refused::Failed) => {
                    return Err(windows::Win32::Foundation::E_FAIL.into());
                }
            };
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let stream = unsafe {
                SHCreateStreamOnFileEx(
                    PCWSTR(wide.as_ptr()),
                    windows::Win32::System::Com::STGM_READ.0,
                    FILE_ATTRIBUTE_NORMAL.0,
                    false,
                    None,
                )?
            };
            return Ok(STGMEDIUM {
                tymed: TYMED_ISTREAM.0 as u32,
                u: windows::Win32::System::Com::STGMEDIUM_0 {
                    pstm: std::mem::ManuallyDrop::new(Some(stream)),
                },
                pUnkForRelease: std::mem::ManuallyDrop::new(None),
            });
        }

        if wanted == self.formats.preferred && request.tymed & TYMED_HGLOBAL.0 as u32 != 0 {
            let effect: u32 = DROPEFFECT_COPY.0;
            let block = unsafe { to_global(&effect.to_le_bytes())? };
            return Ok(hglobal_medium(block));
        }

        if request.tymed & TYMED_HGLOBAL.0 as u32 != 0 {
            if let Some(copy) = self.kept_copy(wanted) {
                return copy.map(hglobal_medium);
            }
        }

        Err(DV_E_FORMATETC.into())
    }

    fn GetDataHere(&self, _f: *const FORMATETC, _m: *mut STGMEDIUM) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, request: *const FORMATETC) -> HRESULT {
        let request = unsafe { *request };
        let ours = request.cfFormat == self.formats.descriptor
            || request.cfFormat == self.formats.contents
            || request.cfFormat == self.formats.preferred;
        if !ours && !self.keeps(request.cfFormat) {
            return DV_E_FORMATETC;
        }
        let tymed = if request.cfFormat == self.formats.contents {
            TYMED_ISTREAM.0 as u32
        } else {
            TYMED_HGLOBAL.0 as u32
        };
        if request.tymed & tymed == 0 {
            return DV_E_TYMED;
        }
        S_OK
    }

    fn GetCanonicalFormatEtc(&self, _i: *const FORMATETC, out: *mut FORMATETC) -> HRESULT {
        unsafe { (*out).ptd = std::ptr::null_mut() };
        DATA_S_SAMEFORMATETC
    }

    /// The drag helpers park the drag image here, in formats of their own,
    /// and read it back from the other side of the drop; refusing them is
    /// what leaves a drag with no picture. Whatever else the shell sets, such
    /// as the effect it performed, is kept the same way.
    fn SetData(
        &self,
        request: *const FORMATETC,
        medium: *const STGMEDIUM,
        release: BOOL,
    ) -> Result<()> {
        let request = unsafe { *request };
        let given = unsafe { &*medium };
        if given.tymed != TYMED_HGLOBAL.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        let handle = unsafe { given.u.hGlobal };
        // A block handed over with nothing else attached is taken as it is.
        // Otherwise it is copied, and one that was handed over is let go of
        // the way its owner asked, through `pUnkForRelease` when there is one.
        let owned = if release.as_bool() && given.pUnkForRelease.is_none() {
            handle
        } else {
            let copy = unsafe {
                OleDuplicateData(
                    HANDLE(handle.0),
                    windows::Win32::System::Ole::CLIPBOARD_FORMAT(request.cfFormat),
                    GMEM_MOVEABLE,
                )
            };
            if release.as_bool() {
                unsafe { ReleaseStgMedium(medium.cast_mut()) };
            }
            if copy.is_invalid() {
                return Err(windows::core::Error::from_win32());
            }
            HGLOBAL(copy.0)
        };
        let mut kept = self
            .kept
            .lock()
            .map_err(|_| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
        if let Some(old) = kept.iter_mut().find(|k| k.format == request.cfFormat) {
            unsafe {
                let _ = GlobalFree(HGLOBAL(old.handle as *mut _));
            }
            old.handle = owned.0 as isize;
        } else {
            kept.push(Kept {
                format: request.cfFormat,
                handle: owned.0 as isize,
            });
        }
        Ok(())
    }

    fn EnumFormatEtc(&self, direction: u32) -> Result<IEnumFORMATETC> {
        if DATADIR(direction as i32) != DATADIR_GET {
            return Err(E_NOTIMPL.into());
        }
        let entry = |format: u16, tymed: u32| FORMATETC {
            cfFormat: format,
            ptd: std::ptr::null_mut(),
            dwAspect: 1,
            lindex: -1,
            tymed,
        };
        let mut offered = vec![
            entry(self.formats.descriptor, TYMED_HGLOBAL.0 as u32),
            entry(self.formats.contents, TYMED_ISTREAM.0 as u32),
            entry(self.formats.preferred, TYMED_HGLOBAL.0 as u32),
        ];
        if let Ok(kept) = self.kept.lock() {
            offered.extend(kept.iter().map(|k| entry(k.format, TYMED_HGLOBAL.0 as u32)));
        }
        // The shell will build the enumerator; there is nothing to be gained
        // from writing a third COM object to walk an array.
        unsafe { SHCreateStdEnumFmtEtc(&offered) }
    }

    fn DAdvise(&self, _f: *const FORMATETC, _a: u32, _s: Option<&IAdviseSink>) -> Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _c: u32) -> Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

/// Lets the Explorer take the entries on its own background thread, with its
/// own progress window, instead of holding the drop until every one has been
/// extracted. It is what its own ZIP folders do.
impl IDataObjectAsyncCapability_Impl for Source_Impl {
    fn SetAsyncMode(&self, asynchronous: BOOL) -> Result<()> {
        self.asynchronous
            .store(asynchronous.as_bool(), Ordering::SeqCst);
        Ok(())
    }

    fn GetAsyncMode(&self) -> Result<BOOL> {
        Ok(self.asynchronous.load(Ordering::SeqCst).into())
    }

    fn StartOperation(&self, _reserved: Option<&IBindCtx>) -> Result<()> {
        self.ending.started.store(true, Ordering::SeqCst);
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn InOperation(&self) -> Result<BOOL> {
        Ok(self.running.load(Ordering::SeqCst).into())
    }

    fn EndOperation(
        &self,
        result: HRESULT,
        _reserved: Option<&IBindCtx>,
        _effect: u32,
    ) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);
        self.ending.finish(result.is_ok());
        Ok(())
    }
}

/// A pointer handed from one thread to another, inside a stream COM made for
/// exactly that crossing.
struct Crossing(usize);

/// Starts a drag of archive entries and returns once they have been dropped or
/// the drag has been abandoned.
///
/// The data object lives in the multithreaded apartment, on a thread of its
/// own, so the calls the shell makes to extract each entry are served by COM's
/// threads and never by the window's: a large entry does not freeze the window
/// while it is being taken. With the async capability the Explorer takes them
/// after the drop, with its own progress window, and this returns at once.
///
/// `deliver` is asked for one item at a time, by index, from whichever thread
/// the shell's call arrives on. `ended` is called exactly once, when the drop
/// is over, with whether it went through; for an async drop that is long after
/// this returns. Entries are only ever copied out: moving would mean taking
/// them out of the archive.
pub fn drag(items: Vec<Item>, deliver: Deliver, ended: Ended) -> Effect {
    let ending = Arc::new(Ending {
        started: AtomicBool::new(false),
        call: Mutex::new(Some(ended)),
    });
    if items.is_empty() {
        ending.finish(false);
        return Effect::None;
    }
    let (ready_tx, ready_rx) = channel::<Option<Crossing>>();
    let (gone_tx, gone_rx) = channel::<()>();
    let shared = ending.clone();
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let source: IDataObject = Source {
            items,
            deliver,
            formats: Formats::get(),
            kept: Mutex::new(Vec::new()),
            asynchronous: AtomicBool::new(true),
            running: AtomicBool::new(false),
            ending: shared,
            gone: Mutex::new(Some(gone_tx)),
        }
        .into();
        let marshaled = CoMarshalInterThreadInterfaceInStream(&IDataObject::IID, &source);
        // The stream holds what the other thread needs; this thread only has
        // to keep the apartment alive until the last reference is gone.
        drop(source);
        match marshaled {
            Ok(stream) => {
                let _ = ready_tx.send(Some(Crossing(stream.into_raw() as usize)));
                let _ = gone_rx.recv();
            }
            Err(_) => {
                let _ = ready_tx.send(None);
            }
        }
        CoUninitialize();
    });

    let Ok(Some(Crossing(raw))) = ready_rx.recv() else {
        return Effect::None;
    };
    let effect = with_ole(|| unsafe {
        let stream = IStream::from_raw(raw as *mut _);
        let data: Result<IDataObject> = CoGetInterfaceAndReleaseStream(&stream);
        // The call above has released the stream itself.
        std::mem::forget(stream);
        let Ok(data) = data else {
            return Effect::None;
        };
        // The shell's own drop source and drag image: the same picture and
        // cursors a drag out of the Explorer has.
        SHDoDragDrop(None, &data, None::<&IDropSource>, DROPEFFECT_COPY)
            .map(effect_of)
            .unwrap_or(Effect::None)
    });
    // A target that took the entries during the drop, rather than after it,
    // is done with them by now.
    if !ending.started.load(Ordering::SeqCst) {
        ending.finish(effect != Effect::None);
    }
    effect
}

/// Runs `f` with OLE initialized on this thread. The window toolkit already
/// does this to be a drop target, and asking again only counts up; without it
/// on a thread that has not, a drag fails outright.
fn with_ole<T>(f: impl FnOnce() -> T) -> T {
    let ours = unsafe { OleInitialize(None) }.is_ok();
    let out = f();
    if ours {
        unsafe { OleUninitialize() };
    }
    out
}

/// Starts a drag of files that already exist, the way the Explorer does: the
/// shell builds the data object from the paths, with its own formats, drag
/// image and cursors, and whoever receives the drop does the copying with its
/// own progress and its own questions. Nothing is read here, however large the
/// folders are. Returns once the drop is over.
pub fn drag_files(paths: &[PathBuf]) -> Effect {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        BHID_DataObject, IShellItemArray, SHCreateShellItemArrayFromIDLists, SHParseDisplayName,
    };

    with_ole(|| unsafe {
        let mut pidls: Vec<*mut ITEMIDLIST> = Vec::with_capacity(paths.len());
        for path in paths {
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let mut pidl = std::ptr::null_mut();
            if SHParseDisplayName(PCWSTR(wide.as_ptr()), None::<&IBindCtx>, &mut pidl, 0, None)
                .is_ok()
                && !pidl.is_null()
            {
                pidls.push(pidl);
            }
        }
        let listed: Vec<*const ITEMIDLIST> = pidls.iter().map(|p| p.cast_const()).collect();
        let array: Result<IShellItemArray> = if listed.is_empty() {
            Err(windows::Win32::Foundation::E_INVALIDARG.into())
        } else {
            SHCreateShellItemArrayFromIDLists(&listed)
        };
        for pidl in pidls {
            CoTaskMemFree(Some(pidl.cast_const().cast()));
        }
        let Ok(data) = array.and_then(|array| {
            array.BindToHandler::<_, IDataObject>(None::<&IBindCtx>, &BHID_DataObject)
        }) else {
            return Effect::None;
        };
        let allowed = DROPEFFECT(DROPEFFECT_COPY.0 | DROPEFFECT_MOVE.0 | DROPEFFECT_LINK.0);
        SHDoDragDrop(None, &data, None::<&IDropSource>, allowed)
            .map(effect_of)
            .unwrap_or(Effect::None)
    })
}
