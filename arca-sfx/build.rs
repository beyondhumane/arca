// Embeds Arca's icon, so a self-extractor shows it in the Explorer instead of
// the generic program icon.
fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=../brand/arca-monolito.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../brand/arca-monolito.ico");
        res.set("FileDescription", "Arca self-extracting archive");
        res.set("ProductName", "Arca");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}
