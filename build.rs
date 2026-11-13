use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    println!("cargo:rerun-if-changed=assets/shard.svg");
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let svg = fs::read_to_string("assets/shard.svg").unwrap();
    for (name, purple, cyan) in [
        ("forest", "#96c969", "#d3f0ab"),
        ("onyx", "#a3a4bd", "#ecedf5"),
        ("pick_me", "#ec8bbd", "#ffd2e7"),
        ("cosmic", "#ad79f4", "#80e4e3"),
    ] {
        fs::write(
            out.join(format!("shard-{name}.svg")),
            svg.replace("#ad79f4", purple)
                .replace("#987fe8", purple)
                .replace("#80e4e3", cyan),
        )
        .unwrap();
    }
    let icon = make_icon(&out, "shard", &svg);
    let mut resources = format!(
        "101 ICON \"{}\"\n",
        icon.display().to_string().replace('\\', "/")
    );
    for (index, name) in ["previous", "play", "pause", "next"]
        .into_iter()
        .enumerate()
    {
        let source = format!("assets/taskbar/{name}.svg");
        println!("cargo:rerun-if-changed={source}");
        let icon = make_icon(&out, name, &fs::read_to_string(source).unwrap());
        resources.push_str(&format!(
            "{} ICON \"{}\"\n",
            102 + index,
            icon.display().to_string().replace('\\', "/")
        ));
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var_os("CARGO_FEATURE_DESKTOP").is_some()
    {
        let resource = out.join("shard.rc");
        resources.push_str("1 VERSIONINFO\nFILEVERSION 0,1,0,0\nPRODUCTVERSION 0,1,0,0\nBEGIN\n BLOCK \"StringFileInfo\"\n BEGIN\n  BLOCK \"040904b0\"\n  BEGIN\n   VALUE \"ProductName\", \"Shard\\0\"\n   VALUE \"FileDescription\", \"Shard Music Player\\0\"\n   VALUE \"OriginalFilename\", \"shard.exe\\0\"\n  END\n END\n BLOCK \"VarFileInfo\"\n BEGIN\n  VALUE \"Translation\", 0x409, 1200\n END\nEND\n");
        fs::write(&resource, resources).unwrap();
        embed_resource::compile_for_everything(resource, embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
}

fn make_icon(out: &Path, name: &str, svg: &str) -> PathBuf {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).unwrap();
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let mut pngs = Vec::new();
    for size in sizes {
        let mut pixels = resvg::tiny_skia::Pixmap::new(size, size).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(
                size as f32 / tree.size().width(),
                size as f32 / tree.size().height(),
            ),
            &mut pixels.as_mut(),
        );
        let png = pixels.encode_png().unwrap();
        if size == 256 {
            fs::write(out.join(format!("{name}.png")), &png).unwrap();
        }
        pngs.push(png);
    }
    let mut ico = vec![0, 0, 1, 0, sizes.len() as u8, 0];
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (size, png) in sizes.into_iter().zip(&pngs) {
        ico.extend_from_slice(&[size as u8, size as u8, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in pngs {
        ico.extend(png);
    }
    let icon = out.join(format!("{name}.ico"));
    fs::write(&icon, ico).unwrap();
    icon
}
