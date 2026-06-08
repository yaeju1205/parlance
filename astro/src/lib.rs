use std::{
    fs, io,
    path::{Path, PathBuf},
};

use parlance_module::{Module, Par, Parable, Pars};

const MODULE_EXTENSION: &str = "par";
const MANIFEST: &str = "parlance.toml";

fn package_root(entry: &Path) -> PathBuf {
    let start = entry.parent().unwrap_or(Path::new("."));
    let mut dir = start;
    loop {
        if dir.join(MANIFEST).is_file() {
            return dir.to_path_buf();
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return start.to_path_buf(),
        }
    }
}

fn collect_par_files(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_par_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == MODULE_EXTENSION) {
            out.push(path);
        }
    }
    Ok(())
}

fn module_path(root: &Path, file: &Path) -> Vec<String> {
    let relative = file.strip_prefix(root).unwrap_or(file);
    let mut segments: Vec<String> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    if let Some(last) = segments.last_mut() {
        if let Some(stripped) = last.strip_suffix(&format!(".{MODULE_EXTENSION}")) {
            *last = stripped.to_string();
        }
    }
    segments
}

pub fn pack(entry: &Path) -> io::Result<Pars> {
    let root = package_root(entry);
    let entry = fs::canonicalize(entry).unwrap_or_else(|_| entry.to_path_buf());

    let mut files = Vec::new();
    collect_par_files(&root, &mut files)?;
    files.sort();

    let mut pars = Vec::with_capacity(files.len());
    let mut entry_index = None;
    for file in &files {
        let canonical = fs::canonicalize(file).unwrap_or_else(|_| file.clone());
        if canonical == entry {
            entry_index = Some(pars.len());
        }
        pars.push(Par {
            module: Module {
                path: module_path(&root, file),
            },
            parable: Parable::Source(fs::read_to_string(file)?),
        });
    }

    let entry = entry_index.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "entry {} is not a .par file under {}",
                entry.display(),
                root.display()
            ),
        )
    })?;

    Ok(Pars { pars, entry })
}

pub fn write_pars(pars: &Pars, out: &Path) -> io::Result<()> {
    let bytes = pars
        .to_bytes()
        .map_err(|err| io::Error::other(format!("failed to serialize pars: {err}")))?;
    fs::write(out, bytes)
}
