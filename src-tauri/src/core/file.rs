use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Error as IOError, Read, Write};
use std::path::PathBuf;
use std::time::UNIX_EPOCH;
use std::{fs::File, path::Path};

use chardetng::EncodingDetector;
use chrono::{Local, TimeZone};
use encoding_rs::{Encoding, UTF_8};
use sheets_diff::core::diff::Diff;
use sheets_diff::core::unified_format::{unified_diff, SplitUnifiedDiffContent};

use super::diff::binary_comparison_only;
use super::str::bytes_to_hex_dump;
use super::types::{FileAttr, ListDirResponse, ReadContent};

/// default charset
const UTF8_CHARSET: &str = "UTF-8";
/// label text on charset on non text file
const NOT_TEXTFILE_CHARSET: &str = "(bytes array)";

/// validate file path to compare
pub fn validate_filepath(filepath: &str) -> Option<bool> {
    if !Path::new(filepath).exists() {
        return None;
    }
    Some(is_textfile(filepath) || filepath.ends_with(".xlsx"))
}

/// get content from file paths on old file and new file
pub fn filepaths_content(old: &str, new: &str) -> Result<Vec<ReadContent>, String> {
    let old_is_textfile = is_textfile(old);
    let new_is_textfile = is_textfile(new);
    if old_is_textfile && new_is_textfile {
        return Ok(vec![textfile_content(old), textfile_content(new)]);
    }
    if old_is_textfile && new.is_empty() {
        return Ok(vec![textfile_content(old), ReadContent::default()]);
    }
    if old.is_empty() && new_is_textfile {
        return Ok(vec![ReadContent::default(), textfile_content(new)]);
    }

    if old.ends_with(".xlsx") && new.ends_with(".xlsx") {
        let diff = Diff::new(old, new);
        let split_unified_diff = unified_diff(&diff).split();
        return Ok(vec![
            excel_content(&split_unified_diff.old),
            excel_content(&split_unified_diff.new),
        ]);
    }

    Ok(vec![binary_content(old), binary_content(new)])
}

/// list files and directories in directory
pub fn list_dir(current_dir: &str) -> Result<ListDirResponse, String> {
    let target_dir = match target_dir(current_dir) {
        Ok(x) => x,
        Err(err) => return Err(err.to_string()),
    };

    let mut dirs = Vec::<String>::new();
    let mut files = Vec::<FileAttr>::new();

    let read = match std::fs::read_dir(target_dir.as_path()) {
        Ok(x) => x,
        Err(err) => {
            return Err(format!("Invalid path: {} ({})", current_dir, err));
        }
    };
    for x in read {
        match x {
            Ok(dir_entry) => {
                let name = dir_entry.file_name().to_string_lossy().to_string();
                match dir_entry.metadata() {
                    Ok(metadata) => {
                        if metadata.is_dir() {
                            dirs.push(name);
                            continue;
                        }

                        let modified = metadata
                            .modified()
                            .unwrap()
                            .duration_since(UNIX_EPOCH)
                            .unwrap();
                        let local_timestamp = Local.timestamp_nanos(modified.as_nanos() as i64);
                        let last_modified = local_timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
                        files.push(FileAttr {
                            name,
                            bytes_size: format!("{} bytes", comma_separated_number(metadata.len())),
                            human_readable_size: human_readable_size(metadata.len()),
                            last_modified,
                            binary_comparison_only: binary_comparison_only(
                                &dir_entry.path().to_string_lossy(),
                            ),
                        })
                    }
                    _ => {}
                }
            }
            // todo
            Err(err) => println!("Failed to get dir/file info due to {}", err),
        }
    }

    dirs.sort();
    files.sort();

    Ok(ListDirResponse {
        current_dir: target_dir.to_string_lossy().to_string(),
        dirs: dirs,
        files: files,
    })
}

/// save to file
pub fn save(filepath: &str, content: &str, charset: &str) -> Result<(), IOError> {
    let encoding = Encoding::for_label(charset.as_bytes()).unwrap_or(UTF_8);
    let (encoded, _, _) = encoding.encode(content);
    let mut file = File::create(filepath)?;
    file.write_all(&encoded)?;
    Ok(())
}

/// command to run file manager
pub fn file_manager_command() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "explorer"
    }
    #[cfg(target_os = "macos")]
    {
        "open"
    }
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        if Command::new("nautilus").arg("--version").output().is_ok() {
            "nautilus"
        } else if Command::new("dolphin").arg("--version").output().is_ok() {
            "dolphin"
        } else if Command::new("nemo").arg("--version").output().is_ok() {
            "nemo"
        } else if Command::new("thunar").arg("--version").output().is_ok() {
            "thunar"
        } else {
            "xdg-open"
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        compile_error!("Unsupported operating system")
    }
}

/// convert executable argument to file path
pub fn arg_to_filepath(arg: &Option<OsString>) -> Option<String> {
    if let Some(s) = arg {
        let s = s.to_string_lossy();
        if Path::new(s.as_ref()).is_file() {
            Some(s.into_owned())
        } else {
            None
        }
    } else {
        None
    }
}

/// check if file is text file
fn is_textfile(filepath: &str) -> bool {
    let file = File::open(filepath);
    match file {
        Ok(f) => {
            let mut reader = BufReader::new(f);
            let mut buffer = String::new();
            reader.read_line(&mut buffer).is_ok()
        }
        Err(_) => false,
    }
}

/// get content from text file
fn textfile_content(filepath: &str) -> ReadContent {
    let mut file = File::open(filepath).expect(format!("failed to open {}", filepath).as_str());
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();

    let is_binary = buffer.windows(2).any(|window| window[0] == 0x00);
    if is_binary {
        const BYTES_ARRAY_ROW_LENGTH: usize = 16;
        let mut grid = String::new();
        for chunk in buffer.chunks(BYTES_ARRAY_ROW_LENGTH) {
            for byte in chunk {
                grid.push_str(&format!("{:02X} ", byte));
            }
            grid.push_str("\n");
        }
        return ReadContent {
            charset: NOT_TEXTFILE_CHARSET.to_owned(),
            content: grid,
        };
    }

    match std::str::from_utf8(&buffer) {
        Ok(x) => {
            return ReadContent {
                charset: UTF8_CHARSET.to_owned(),
                content: x.to_owned(),
            }
        }
        Err(_) => (),
    }

    let mut detector = EncodingDetector::new();
    detector.feed(&buffer, true);
    let encoding = detector.guess(None, false);
    let (decoded, _, had_errors) = encoding.decode(&buffer);
    if had_errors {
        eprint!("not binary, not utf-8 text and not any other encoded text.")
    }
    ReadContent {
        charset: encoding.name().to_owned(),
        content: decoded.to_string(),
    }
}

/// read content from ms excel
fn excel_content(split_unified_diff_content: &Vec<SplitUnifiedDiffContent>) -> ReadContent {
    let content = split_unified_diff_content
        .iter()
        .map(|x| {
            let mut ret: Vec<String> = vec![x.title.to_owned()];
            ret.extend(x.lines.iter().flat_map(|x| {
                let mut ret: Vec<String> = vec![];
                if let Some(pos) = &x.pos {
                    ret.push(pos.to_owned());
                }
                if let Some(text) = &x.text {
                    ret.push(text.to_owned());
                }
                ret
            }));
            ret.join("\n")
        })
        .collect();
    ReadContent {
        charset: "(Excel)".to_owned(),
        content,
    }
}

/// read content as bynary
fn binary_content(filepath: &str) -> ReadContent {
    let read_bytes = fs::read(Path::new(filepath)).expect("Failed to read file in binary mode");
    let hex_dump = bytes_to_hex_dump(&read_bytes);
    ReadContent {
        charset: "(binary)".to_owned(),
        content: hex_dump,
    }
}

/// convert pathbuf to os dependent one
fn os_path_buf(path_buf: &PathBuf) -> PathBuf {
    #[cfg(not(target_os = "windows"))]
    {
        path_buf.to_owned()
    }
    #[cfg(target_os = "windows")]
    {
        // extended-length path prefix sometimes appears on windows
        const WINDOWS_EXTENDED_LENGTH_PATH_PREFIX: &str = r"\\?\";

        let windows_path_buf = path_buf
            .to_str()
            .expect("Failed to convert current_dir to string");
        if windows_path_buf.starts_with(WINDOWS_EXTENDED_LENGTH_PATH_PREFIX) {
            PathBuf::from(&windows_path_buf[WINDOWS_EXTENDED_LENGTH_PATH_PREFIX.len()..])
        } else {
            windows_path_buf.into()
        }
    }
}

/// target dir
fn target_dir(current_dir: &str) -> Result<PathBuf, IOError> {
    let ret = if current_dir.is_empty() {
        std::env::current_dir().expect("Failed to get current directory")
    } else {
        Path::new(current_dir).canonicalize()?
    };
    Ok(os_path_buf(&ret))
}

/// add separator commnas to number
fn comma_separated_number(num: u64) -> String {
    let num_str = num.to_string();

    let mut ret = String::new();
    for (i, c) in num_str.chars().rev().enumerate() {
        if i != 0 && i % 3 == 0 {
            ret.push(',');
        }
        ret.push(c);
    }

    ret.chars().rev().collect()
}

/// convert file size to human readable number
fn human_readable_size(size: u64) -> String {
    const UNIT: u64 = 1024;
    const K: u64 = UNIT;
    const M: u64 = UNIT.pow(2);
    const G: u64 = UNIT.pow(3);
    const T: u64 = UNIT.pow(4);

    let (size, unit) = if size >= T {
        (size as f64 / T as f64, "TB")
    } else if size >= G {
        (size as f64 / G as f64, "GB")
    } else if size >= M {
        (size as f64 / M as f64, "MB")
    } else if size >= K {
        (size as f64 / K as f64, "KB")
    } else {
        (size as f64, "bytes")
    };

    let size_str = size.to_string();
    let size_str_parts = size_str.split(".").collect::<Vec<&str>>();
    let int = size_str_parts[0].parse::<u64>().unwrap();
    let comma_separated_int = comma_separated_number(int);

    let comma_separated_size = if 1 < size_str_parts.len() {
        format!("{}.{:.2}", comma_separated_int, size_str_parts[1])
    } else {
        comma_separated_int
    };
    format!("{} {}", comma_separated_size, unit)
}
