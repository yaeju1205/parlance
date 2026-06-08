use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

use parlance_module::{FileContent, Pars, VirtualFile};
use serde::Deserialize;

const MODULE_EXTENSION: &str = "par";
const MANIFEST: &str = "astro.toml";
const DEFAULT_SRC: &str = "src";

#[derive(Deserialize)]
struct ManifestPackage {
    name: String,
    #[serde(default)]
    main: Option<String>,
    #[serde(default)]
    src: Option<String>,
}

#[derive(Deserialize)]
struct Dependency {
    path: String,
}

#[derive(Deserialize)]
struct Manifest {
    package: ManifestPackage,
    #[serde(default)]
    dependencies: HashMap<String, Dependency>,
}

struct Package {
    name: String,
    root: PathBuf,
    src: PathBuf,
    dependencies: Vec<(String, PathBuf)>,
}

/// Walk up from `start` to the nearest directory containing an `astro.toml`.
fn find_manifest_dir(start: &Path) -> io::Result<PathBuf> {
    let mut dir = start;
    loop {
        if dir.join(MANIFEST).is_file() {
            return Ok(dir.to_path_buf());
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("no {MANIFEST} found above {}", start.display()),
                ));
            }
        }
    }
}

fn read_manifest(root: &Path) -> io::Result<Manifest> {
    let manifest_path = root.join(MANIFEST);
    let content = fs::read_to_string(&manifest_path)?;
    toml::from_str(&content).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid manifest {}: {err}", manifest_path.display()),
        )
    })
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

fn module_segments(root: &Path, file: &Path) -> Vec<String> {
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

/// Recursively load the dependency graph rooted at `root`, deduping by
/// canonical root (which also breaks cycles). Validates that each dependency
/// key matches the dependency package's declared name, and that package names
/// are unique across the graph.
fn load_packages(root: &Path) -> io::Result<Vec<Package>> {
    let mut packages: Vec<Package> = Vec::new();
    let mut visited: HashMap<PathBuf, ()> = HashMap::new();
    let mut by_name: HashMap<String, PathBuf> = HashMap::new();
    let mut queue = vec![fs::canonicalize(root)?];

    while let Some(root) = queue.pop() {
        if visited.contains_key(&root) {
            continue;
        }

        let manifest = read_manifest(&root)?;
        let name = manifest.package.name.clone();
        let src = root.join(manifest.package.src.as_deref().unwrap_or(DEFAULT_SRC));

        if let Some(existing) = by_name.get(&name) {
            if existing != &root {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "duplicate package name '{name}': {} and {}",
                        existing.display(),
                        root.display()
                    ),
                ));
            }
        }
        by_name.insert(name.clone(), root.clone());
        visited.insert(root.clone(), ());

        let mut dependencies = Vec::new();
        for (key, dependency) in &manifest.dependencies {
            let dep_root = fs::canonicalize(root.join(&dependency.path)).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "dependency '{key}' path '{}' (from {}): {err}",
                        dependency.path,
                        root.display()
                    ),
                )
            })?;

            let dep_name = read_manifest(&dep_root)?.package.name;
            if dep_name != *key {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "dependency '{key}' points at a package named '{dep_name}'; \
                         the dependency name must match the package name"
                    ),
                ));
            }

            dependencies.push((key.clone(), dep_root.clone()));
            queue.push(dep_root);
        }

        packages.push(Package {
            name,
            root,
            src,
            dependencies,
        });
    }

    Ok(packages)
}

fn virtual_root(name: &str) -> String {
    format!("/{name}")
}

/// Build the bundle's `astro.toml` for a package, wiring each dependency to its
/// virtual root so the resolver can follow it inside the bundle.
fn synthesized_manifest(package: &Package) -> String {
    let mut manifest = format!("[package]\nname = \"{}\"\n", package.name);
    if !package.dependencies.is_empty() {
        manifest.push_str("\n[dependencies]\n");
        for (key, _) in &package.dependencies {
            manifest.push_str(&format!("{key} = {{ path = \"{}\" }}\n", virtual_root(key)));
        }
    }
    manifest
}

/// Resolve the default entry from the `main` field of the nearest `astro.toml`
/// (searching up from `dir`). `main` is interpreted relative to the source dir.
pub fn default_entry(dir: &Path) -> io::Result<PathBuf> {
    let root = find_manifest_dir(dir)?;
    let manifest = read_manifest(&root)?;
    let main = manifest.package.main.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "no 'main' declared in {}; pass an entry file explicitly",
                root.join(MANIFEST).display()
            ),
        )
    })?;
    let src = manifest.package.src.as_deref().unwrap_or(DEFAULT_SRC);
    Ok(root.join(src).join(main))
}

pub fn pack(entry: &Path) -> io::Result<Pars> {
    let entry = fs::canonicalize(entry)?;
    let entry_dir = entry.parent().unwrap_or(Path::new("."));
    let entry_root = fs::canonicalize(find_manifest_dir(entry_dir)?)?;

    let packages = load_packages(&entry_root)?;

    let mut entry_path = None;
    let mut files = Vec::new();

    for package in &packages {
        let root = virtual_root(&package.name);

        files.push(VirtualFile {
            path: format!("{root}/{MANIFEST}"),
            content: FileContent::Source(synthesized_manifest(package)),
        });

        if !package.src.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "package '{}' has no source directory {}",
                    package.name,
                    package.src.display()
                ),
            ));
        }

        let mut par_files = Vec::new();
        collect_par_files(&package.src, &mut par_files)?;
        par_files.sort();

        for file in &par_files {
            let segments = module_segments(&package.src, file).join("/");
            let virtual_path = format!("{root}/{segments}.{MODULE_EXTENSION}");
            if package.root == entry_root && fs::canonicalize(file)? == entry {
                entry_path = Some(virtual_path.clone());
            }
            files.push(VirtualFile {
                path: virtual_path,
                content: FileContent::Source(fs::read_to_string(file)?),
            });
        }
    }

    let entry = entry_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "entry {} is not a .par file under the source dir of {}",
                entry.display(),
                entry_root.display()
            ),
        )
    })?;

    Ok(Pars { files, entry })
}

pub fn write_pars(pars: &Pars, out: &Path) -> io::Result<()> {
    let bytes = pars
        .to_bytes()
        .map_err(|err| io::Error::other(format!("failed to serialize pars: {err}")))?;
    fs::write(out, bytes)
}
