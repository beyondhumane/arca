use super::*;

fn disk() -> Vec<u8> {
    let mut disk = mbr(&[(0x0e, &sample_fat(12))], None);
    disk.resize(disk.len() + 200 * 1024, 0);
    disk
}

#[test]
fn a_vhdx_holds_the_disk_and_reads_absent_blocks_as_zeros() {
    let disk = disk();
    let (_dir, a) = open(Format::Vhdx, &vhdx(&disk, false));
    check_sample(&a.unwrap(), "0.fat");
    let mut sparse = vec![0u8; 3 * 1024 * 1024];
    sparse[2 * 1024 * 1024 + 5] = 1;
    let (_dir, a) = open(Format::Vhdx, &vhdx(&sparse, false));
    assert_eq!(read(&a.unwrap(), "disk.img"), sparse);
}

#[test]
fn a_vhdx_with_a_log_to_replay_is_refused() {
    let (_dir, a) = open(Format::Vhdx, &vhdx(&disk(), true));
    let Err(Error::Unsupported(message)) = a else {
        panic!("a dirty VHDX must be refused");
    };
    assert!(message.contains("log"));
}

#[test]
fn a_vhdx_survives_one_damaged_header() {
    let mut image = vhdx(&disk(), false);
    image[128 * 1024 + 100] ^= 1;
    let (_dir, a) = open(Format::Vhdx, &image);
    check_sample(&a.unwrap(), "0.fat");
}

#[test]
fn damaged_vhdxs_never_panic() {
    survives_damage_at(Format::Vhdx, &vhdx(&disk(), false), &vhdx_hot_spots());
}

#[test]
fn sparse_and_stream_optimized_vmdks_hold_the_disk() {
    let disk = disk();
    for compressed in [false, true] {
        let (_dir, a) = open(Format::Vmdk, &vmdk_sparse(&disk, compressed, "ffffffff"));
        check_sample(&a.unwrap(), "0.fat");
    }
    let (_dir, a) = open(Format::Vmdk, &vmdk_sparse(&disk, false, "0badf00d"));
    assert!(matches!(a, Err(Error::Unsupported(_))));
}

#[test]
fn a_flat_vmdk_reads_its_extent_next_to_the_descriptor() {
    let disk = disk();
    let dir = tempfile::tempdir().unwrap();
    let sectors = disk.len() / 512;
    let descriptor = format!(
        "# Disk DescriptorFile\nversion=1\nCID=fffffffe\nparentCID=ffffffff\ncreateType=\"monolithicFlat\"\n\n# Extent description\nRW {} FLAT \"guest-flat.vmdk\" 0\nRW 8 ZERO\n",
        sectors
    );
    stdfs::write(dir.path().join("guest.vmdk"), descriptor).unwrap();
    stdfs::write(dir.path().join("guest-flat.vmdk"), &disk).unwrap();
    let a = DiskArchive::open(&dir.path().join("guest.vmdk"), Format::Vmdk).unwrap();
    check_sample(&a, "0.fat");
    let escape = "# Disk DescriptorFile\nRW 8 FLAT \"../secret\" 0\n";
    stdfs::write(dir.path().join("escape.vmdk"), escape).unwrap();
    assert!(matches!(
        DiskArchive::open(&dir.path().join("escape.vmdk"), Format::Vmdk),
        Err(Error::Unsupported(_))
    ));
}

#[test]
fn damaged_vmdks_never_panic() {
    let disk = disk();
    let plain = vmdk_sparse(&disk, false, "ffffffff");
    let spots: Vec<usize> = (0..plain.len())
        .filter(|&at| {
            (at < 512 && at % 3 == 0)
                || (10752..13312).contains(&at) && at % 7 == 0
                || at % 4093 == 0
        })
        .collect();
    survives_damage_at(Format::Vmdk, &plain, &spots);
    let packed = vmdk_sparse(&disk, true, "ffffffff");
    let mut spots: Vec<usize> = (0..packed.len()).filter(|at| at % 509 == 0).collect();
    spots.extend(
        (0..1536)
            .map(|back| packed.len() - 1 - back)
            .filter(|at| at % 3 == 0),
    );
    survives_damage_at(Format::Vmdk, &packed, &spots);
}

#[test]
fn a_dmg_lists_its_blocks_as_partitions_and_checks_their_sums() {
    let fat = sample_fat(12);
    let image = dmg(&[
        ("Driver Descriptor Map (DDM : 0)", &[0x45; 512]),
        ("MS-DOS FAT12 (DOS_FAT_12 : 1)", &fat),
        ("(Apple_Free : 2)", &[0; 4096]),
    ]);
    let (_dir, a) = open(Format::Dmg, &image);
    let a = a.unwrap();
    check_sample(&a, "1.fat");
    assert_eq!(read(&a, "0.img"), vec![0x45; 512]);
    assert!(!names(&a).iter().any(|n| n.starts_with("2.")));
    a.test(&[], &|_, _, _| true).unwrap();
}

#[test]
fn a_dmg_whose_data_changed_fails_its_test() {
    let image = dmg(&[("disk image", &[0x45; 64 * 512 * 4])]);
    let mut damaged = image.clone();
    let at = image
        .windows(512)
        .position(|w| w.iter().all(|&b| b == 0x45))
        .unwrap();
    damaged[at] = 0x46;
    let (_dir, a) = open(Format::Dmg, &damaged);
    let a = a.unwrap();
    assert!(matches!(
        a.test(&[], &|_, _, _| true),
        Err(Error::Integrity { .. })
    ));
}

#[test]
fn adc_copies_reach_back_into_what_was_written() {
    let coded = [0x82, b'a', b'b', b'c', 0x00, 0x02, 0x40, 0x00, 0x05];
    assert_eq!(dmg::adc(&coded, 100).unwrap(), b"abcabcabca");
    assert!(dmg::adc(&[0x00, 0x05], 10).is_err());
    assert!(dmg::adc(&[0x85, b'a'], 10).is_err());
}

#[test]
fn encrypted_and_damaged_dmgs_are_refused_without_a_panic() {
    let (_dir, a) = open(Format::Dmg, b"encrcdsa\0\0\0\x02rest of it");
    assert!(matches!(a, Err(Error::Unsupported(_))));
    let fat = sample_fat(12);
    let image = dmg(&[("FAT", &fat)]);
    let mut spots: Vec<usize> = (0..image.len()).filter(|at| at % 211 == 0).collect();
    spots.extend((0..600).map(|back| image.len() - 1 - back));
    survives_damage_at(Format::Dmg, &image, &spots);
}
