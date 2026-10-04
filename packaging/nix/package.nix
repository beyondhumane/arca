{
  lib,
  rustPlatform,
  pkg-config,
  copyDesktopItems,
  makeDesktopItem,
  fontconfig,
  freetype,
  libxkbcommon,
  libxcb,
  libX11,
  wayland,
  libGL,
  vulkan-loader,
}:

let
  cargoToml = lib.importTOML ../../Cargo.toml;
  # wgpu and the Wayland backend dlopen these, so they go on the RPATH
  # instead of being picked up by the linker.
  runtimeLibs = [
    libxkbcommon
    libxcb
    libX11
    wayland
    libGL
    vulkan-loader
  ];
in
rustPlatform.buildRustPackage {
  pname = "arca";
  inherit (cargoToml.workspace.package) version;

  src = lib.fileset.toSource {
    root = ../..;
    fileset = lib.fileset.difference ../.. (
      lib.fileset.unions [
        ../../site
        ../../docs
        ../../windows
        ../../packaging
        ../../flake.lock
        (lib.fileset.maybeMissing ../../target)
        (lib.fileset.fileFilter (f: f.hasExt "nix" || f.hasExt "md") ../..)
      ]
    );
  };

  cargoLock.lockFile = ../../Cargo.lock;

  cargoBuildFlags = [
    "-p"
    "arca-cli"
    "-p"
    "arca-gui"
  ];

  # The suite runs against zip, 7z and tar and takes over ten minutes; CI
  # already runs it on every change.
  doCheck = false;

  nativeBuildInputs = [
    pkg-config
    copyDesktopItems
  ];

  buildInputs = [
    fontconfig
    freetype
  ]
  ++ runtimeLibs;

  desktopItems = [
    (makeDesktopItem {
      name = "arca";
      desktopName = "Arca";
      genericName = "Archive Manager";
      comment = "Create, browse and extract ZIP and TAR archives";
      exec = "arca-gui %f";
      icon = "arca";
      categories = [
        "Utility"
        "Archiving"
        "Compression"
      ];
      mimeTypes = [
        "application/zip"
        "application/x-tar"
        "application/x-compressed-tar"
      ];
      keywords = [
        "zip"
        "tar"
        "archive"
        "extract"
        "compress"
      ];
    })
  ];

  postInstall = ''
    install -Dm644 brand/arca-monolito-256.png $out/share/icons/hicolor/256x256/apps/arca.png
    install -Dm644 RAR-NOTICES.txt $out/share/licenses/arca/RAR-NOTICES.txt
  '';

  postFixup = ''
    patchelf --add-rpath ${lib.makeLibraryPath runtimeLibs} $out/bin/arca-gui
  '';

  passthru = { inherit runtimeLibs; };

  meta = {
    description = "Fast and safe archive manager";
    homepage = "https://arca.beyondhumane.com";
    license = lib.licenses.asl20;
    mainProgram = "arca";
    platforms = lib.platforms.linux;
  };
}
