use std::{
  collections::BTreeMap,
  ffi::OsStr,
  fs::{self, File},
  io::{BufRead, BufReader, BufWriter, Lines, Write},
  iter::{Flatten, Peekable},
  path::Path,
  sync::mpsc::channel,
};

use anyhow::Result;
use clap::Parser;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use time::{format_description::well_known::Iso8601, OffsetDateTime};

#[derive(Debug, Clone)]
enum Prop {
  Str(String),
  Vec(Vec<String>),
}

/// Export mds from Obsidian to Hugo
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
  /// Obsidian vault dir
  #[arg(short, long)]
  obsidian_dir: String,

  /// Hugo dir
  #[arg(short('g'), long)]
  hugo_dir: String,

  /// Hugo posts sub dir
  #[arg(short('p'), long, default_value = "content/posts")]
  hugo_posts_dir: String,

  /// Hugo assets sub dir
  #[arg(short('a'), long, default_value = "content/assets")]
  hugo_assets_dir: String,

  /// Watch
  #[arg(short, long, default_value_t = false)]
  watch: bool,
}

fn main() -> Result<()> {
  let args = Args::parse();

  let obsidian_dir = fs::canonicalize(args.obsidian_dir).expect("Cannot find Obsidian vault dir");
  let hugo_dir = fs::canonicalize(args.hugo_dir).expect("Cannot find hugo dir");

  let src_dir = obsidian_dir;
  let asset_src = src_dir.join("assets");

  let dst_dir = hugo_dir.join(args.hugo_posts_dir);
  let asset_dst = hugo_dir.join(args.hugo_assets_dir);

  if dst_dir.exists() {
    fs::remove_dir_all(&dst_dir)?;
  }
  fs::create_dir(&dst_dir)?;
  if asset_dst.exists() {
    fs::remove_dir_all(&asset_dst)?;
  }
  fs::create_dir(&asset_dst)?;

  recursive_scan(&src_dir, Path::new(""), &|sub_path| {
    export(
      &src_dir.join(sub_path),
      &dst_dir.join(to_url(sub_path.to_str().unwrap())),
      &asset_src,
      &asset_dst,
    )
  })?;

  if !args.watch {
    return Ok(());
  }

  println!("=== \n Watch {src_dir:?} \n===");

  let (tx, rx) = channel();
  let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
  watcher.watch(&src_dir, RecursiveMode::Recursive)?;

  for res in rx {
    match res {
      Ok(event) => match event.kind {
        EventKind::Modify(_) => {
          for full_path in &event.paths {
            let file_name = full_path.file_name().unwrap().to_str().unwrap();
            if file_name.starts_with('.') || file_name.ends_with('~') {
              continue;
            }
            if let Ok(sub_path) = full_path.strip_prefix(&src_dir) {
              export(
                &full_path,
                &dst_dir.join(to_url(sub_path.to_str().unwrap())),
                &asset_src,
                &asset_dst,
              )?;
            }
          }
        }
        _ => {}
      },
      Err(error) => println!("Error: {error:?}"),
    }
  }

  Ok(())
}

fn recursive_scan(base_dir: &Path, sub_dir: &Path, cb: &dyn Fn(&Path) -> Result<()>) -> Result<()> {
  let dir = base_dir.join(sub_dir);

  if dir.is_dir() {
    for res in fs::read_dir(&dir)? {
      let entry = res?;
      let path = entry.path();
      let name = entry.file_name();
      if name.as_encoded_bytes()[0] == b'.' {
        continue;
      }
      let sub_path = sub_dir.join(name);
      if path.is_dir() {
        recursive_scan(base_dir, &sub_path, cb)?;
      } else {
        if Some(OsStr::new("md")) == path.extension() {
          // println!("{sub_path:?}");
          cb(&sub_path)?;
        }
      }
    }
  }
  Ok(())
}

// fn to_hex_path(path: &Path) -> String {
//   let bytes = path.as_os_str().as_encoded_bytes();
//   let mut p = bytes[..bytes.len() - 3]
//     .iter()
//     .map(|b| format!("{:02x}", b))
//     .collect::<Vec<String>>()
//     .join("");
//   p.push_str(".md");
//   p
// }

fn to_url(p: &str) -> String {
  p.replace(" ", "-").replace("/", "-").to_lowercase()
  // let p = path
  //   .to_str()
  //   .unwrap()
  //   .replace(" ", "-")
  //   .replace("/", "-")
  //   .to_lowercase();
  // p
  // form_urlencoded::byte_serialize(p.as_bytes()).collect()
  // let bytes: Vec<u8> = path
  //   .as_os_str()
  //   .to_ascii_lowercase()
  //   .as_encoded_bytes()
  //   .iter()
  //   .map(|b| if b.is_ascii_whitespace() { b'-' } else { *b })
  //   .collect();
  // String::from_utf8_lossy(&bytes).to_string()
}

fn export(src: &Path, dst: &Path, asset_src: &Path, asset_dst: &Path) -> Result<()> {
  let src_file = File::open(src)?;
  let mut src_lines = BufReader::new(src_file).lines().flatten().peekable();

  // Extract src props
  if let Some(src_props) = extract_src_props(&mut src_lines) {
    if !contain_publish_web(&src_props) {
      return Ok(());
    }

    if !is_modified(src, dst) {
      return Ok(());
    }

    println!("\n export: {src:?} \n    -> {dst:?}");

    // Build dst props
    let dst_props = build_dst_props(&src_props, src);

    let dst_file = File::create(dst)?;
    let mut writer = BufWriter::new(dst_file);

    // Write dst props
    writeln!(writer, "---")?;
    for (key, val) in dst_props.iter() {
      match val {
        Prop::Str(s) => {
          writeln!(writer, "{key}: {s}")?;
        }
        Prop::Vec(v) => {
          writeln!(writer, "{key}:")?;
          for item in v {
            writeln!(writer, " - {item}")?;
          }
        }
      }
    }
    writeln!(writer, "---")?;

    // Write content
    let mut is_coding = false;
    for line in src_lines {
      if line.trim().eq("=== end ===") {
        break;
      }

      // Ignore coding blocks
      if !is_coding && line.trim().starts_with("```") {
        is_coding = true;
      }

      if is_coding {
        writeln!(writer, "{line}")?;
        if line.trim().eq("```") {
          is_coding = false;
        }
        continue;
      }

      // Write line by line
      let mut curr = 0;
      // Replace `[[Some title]]` to `[Some tile](/posts/some-title/)`
      // Replace `[[some-img.png]]` to `[some-img.png](/assets/some-img.png)`
      while let Some(start) = line[curr..].find("[[") {
        write!(writer, "{}", &line[curr..(curr + start)])?;
        curr += start;
        if let Some(end) = line[(curr + 2)..].find("]]") {
          let inner = &line[(curr + 2)..(curr + 2 + end)];
          if inner.ends_with(".png") || inner.ends_with(".jpg") {
            let inner_url = to_url(inner);
            let img_src = asset_src.join(inner);
            let img_dst = asset_dst.join(&inner_url);
            println!("    copy: {img_src:?} \n      -> {img_dst:?}");
            fs::copy(img_src, img_dst)?;
            write!(writer, "[{inner_url}](/assets/{inner_url})")?;
          } else if !inner.trim().is_empty() {
            write!(writer, "[{}](/posts/{}/)", inner, to_url(inner))?;
          } else {
            write!(writer, "[[{inner}]]")?;
          }
          curr += 2 + end + 2;
        } else {
          write!(writer, "{}", &line[curr..])?;
          curr = line.len();
        }
      }
      write!(writer, "{}\n", &line[curr..])?;
    }
    writer.flush()?;
  }

  Ok(())
}

fn build_dst_props(src_props: &BTreeMap<String, Prop>, src: &Path) -> BTreeMap<String, Prop> {
  let mut props: BTreeMap<String, Prop> = BTreeMap::new();

  let title = src
    .file_name()
    .unwrap()
    .to_str()
    .unwrap()
    .trim_end_matches(".md");

  props.insert("title".to_string(), Prop::Str(title.to_string()));

  let modified: OffsetDateTime = fs::metadata(src).unwrap().modified().unwrap().into();
  props.insert(
    "date".to_string(),
    Prop::Str(modified.format(&Iso8601::DEFAULT).unwrap()),
  );

  if let Some(tags) = src_props.get("tags") {
    props.insert("tags".to_string(), tags.clone());
  }

  props
}

fn contain_publish_web(props: &BTreeMap<String, Prop>) -> bool {
  if let Some(Prop::Str(v)) = props.get("publish") {
    v.eq("web")
  } else {
    false
  }
}

fn is_modified(src: &Path, dst: &Path) -> bool {
  if !dst.exists() {
    true
  } else {
    let src_modified = fs::metadata(src).unwrap().modified().unwrap();
    let dst_modified = fs::metadata(dst).unwrap().modified().unwrap();
    src_modified.gt(&dst_modified)
  }
}

fn extract_src_props(
  lines: &mut Peekable<Flatten<Lines<BufReader<File>>>>,
) -> Option<BTreeMap<String, Prop>> {
  while let Some(line) = lines.peek() {
    if line.is_empty() {
      lines.next();
    } else {
      break;
    }
  }
  if let Some(line) = lines.peek() {
    if line.trim().eq("---") {
      lines.next();
    } else {
      return None;
    }
  }

  let mut props: BTreeMap<String, Prop> = BTreeMap::new();
  let mut vec_key = String::new();

  while let Some(line) = lines.next() {
    if line.trim().eq("---") {
      break;
    }
    // println!("> {line}");
    if let Some(pos) = line.find(':') {
      let key = line[..pos].trim();
      let val = line[(pos + 1)..].trim();
      if !key.is_empty() && !val.is_empty() {
        if let Some(vec) = str_to_vec(val) {
          props.insert(key.to_string(), Prop::Vec(vec));
        } else {
          props.insert(key.to_string(), Prop::Str(val.to_string()));
        }
        vec_key = "".to_string();
      } else if !key.is_empty() && val.is_empty() {
        vec_key = key.to_string();
        props.insert(key.to_string(), Prop::Vec(vec![]));
      } else {
        vec_key = "".to_string();
      }
    } else if let Some(pos) = line.find('-') {
      let pre = line[..pos].trim();
      if !pre.is_empty() {
        continue;
      }
      if vec_key.is_empty() {
        continue;
      }
      let val = line[(pos + 1)..].trim();
      if val.is_empty() {
        continue;
      }

      if let Some(Prop::Vec(vec)) = props.get_mut(&vec_key) {
        vec.push(val.to_string());
      }
    }
  }
  if props.len() > 0 {
    Some(props)
  } else {
    None
  }
}

fn str_to_vec(val: &str) -> Option<Vec<String>> {
  if val.starts_with('[') && val.ends_with(']') {
    let items = val[1..val.len() - 1]
      .split(',')
      .map(|item| item.trim().trim_matches('"').to_string())
      .collect();
    Some(items)
  } else {
    None
  }
}
