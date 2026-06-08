use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use parlance_diagnostics::{Diagnostics, Span};
use parlance_parser::{
    Export, ExportItem, ImportTarget, ItemSpec, ModulePath, Parser, PathAnchor, Statement,
    StatementKind,
};
use serde::Deserialize;

use crate::desugarer::{DesugarBinding, DesugarValue, DesugarValueKind, desugar};

pub trait ModuleSource {
    fn canonicalize(&self, path: &Path) -> PathBuf;
    fn read_to_string(&self, path: &Path) -> Option<String>;
    fn is_file(&self, path: &Path) -> bool;
}

pub struct FsModuleSource;

impl ModuleSource for FsModuleSource {
    fn canonicalize(&self, path: &Path) -> PathBuf {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn read_to_string(&self, path: &Path) -> Option<String> {
        fs::read_to_string(path).ok()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
}

#[derive(Deserialize)]
struct ManifestPackage {
    #[allow(dead_code)]
    name: String,
}

#[derive(Deserialize)]
struct Manifest {
    #[allow(dead_code)]
    package: ManifestPackage,
    #[serde(default)]
    externs: HashMap<String, String>,
}

struct Package {
    root: PathBuf,
    externs: HashMap<Rc<str>, PathBuf>,
}

enum ResolvedImport {
    Glob { target: usize },
    Items { target: usize, items: Vec<ItemSpec> },
}

enum ResolvedExport {
    Local(Vec<ItemSpec>),
    LocalGlob,
    FromGlob { target: usize },
    FromItems { target: usize, items: Vec<ItemSpec> },
}

struct Module {
    dir: PathBuf,
    package: usize,
    prefix: String,
    bindings: Vec<DesugarBinding>,
    public: HashSet<Rc<str>>,
    imports: Vec<parlance_parser::Import>,
    exports_ast: Vec<Export>,
    resolved_imports: Vec<ResolvedImport>,
    resolved_exports: Vec<ResolvedExport>,
    own: HashMap<Rc<str>, Rc<str>>,
    base_names: HashMap<Rc<str>, Rc<str>>,
    names: HashMap<Rc<str>, Rc<str>>,
    module_aliases: HashMap<Rc<str>, usize>,
    exports: HashMap<Rc<str>, Rc<str>>,
}

pub struct Resolver<'s> {
    source: &'s dyn ModuleSource,
    packages: Vec<Package>,
    modules: Vec<Module>,
    path_to_module: HashMap<PathBuf, usize>,
    root_to_package: HashMap<PathBuf, usize>,
    injected_externs: HashMap<Rc<str>, PathBuf>,
}

fn local_name(item: &ItemSpec) -> Rc<str> {
    item.alias
        .as_ref()
        .map(|node| node.kind.clone())
        .unwrap_or_else(|| item.name.kind.clone())
}

fn statement_name(stat: &Statement) -> Rc<str> {
    match &stat.kind {
        StatementKind::Variable { name, .. } => name.clone(),
        StatementKind::Function { name, .. } => name.clone(),
        StatementKind::Infix { operator, .. } => operator.kind.clone(),
    }
}

impl<'s> Resolver<'s> {
    fn new(source: &'s dyn ModuleSource, injected_externs: HashMap<Rc<str>, PathBuf>) -> Self {
        Self {
            source,
            packages: Vec::new(),
            modules: Vec::new(),
            path_to_module: HashMap::new(),
            root_to_package: HashMap::new(),
            injected_externs,
        }
    }

    fn find_manifest_root(&self, start_dir: &Path) -> Option<PathBuf> {
        let mut dir = start_dir.to_path_buf();
        loop {
            if self.source.is_file(&dir.join("parlance.toml")) {
                return Some(dir);
            }
            match dir.parent() {
                Some(parent) => dir = parent.to_path_buf(),
                None => return None,
            }
        }
    }

    fn ensure_package(&mut self, root: &Path) -> Result<usize, Diagnostics> {
        let root = self.source.canonicalize(root);

        if let Some(&idx) = self.root_to_package.get(&root) {
            return Ok(idx);
        }

        let mut externs: HashMap<Rc<str>, PathBuf> = HashMap::new();

        let manifest_path = root.join("parlance.toml");
        if self.source.is_file(&manifest_path) {
            let content = self.source.read_to_string(&manifest_path).ok_or_else(|| {
                Diagnostics::compiler_error(
                    format!("can not read manifest {}", manifest_path.display()),
                    Span::default(),
                )
            })?;
            let manifest: Manifest = toml::from_str(&content).map_err(|err| {
                Diagnostics::compiler_error(
                    format!("invalid manifest {}: {}", manifest_path.display(), err),
                    Span::default(),
                )
            })?;

            for (name, rel) in manifest.externs {
                externs.insert(
                    Rc::from(name.as_str()),
                    self.source.canonicalize(&root.join(rel)),
                );
            }
        }

        for (name, path) in &self.injected_externs {
            externs.insert(name.clone(), path.clone());
        }

        let idx = self.packages.len();
        self.packages.push(Package {
            root: root.clone(),
            externs,
        });
        self.root_to_package.insert(root, idx);
        Ok(idx)
    }

    fn load_module(
        &mut self,
        file: PathBuf,
        package: usize,
        is_entry: bool,
    ) -> Result<usize, Diagnostics> {
        let file = self.source.canonicalize(&file);

        if let Some(&idx) = self.path_to_module.get(&file) {
            return Ok(idx);
        }

        let source = self.source.read_to_string(&file).ok_or_else(|| {
            Diagnostics::compiler_error(
                format!("can not open module {}", file.display()),
                Span::default(),
            )
        })?;

        let parse_info = Parser::new(&source)?.parse()?;

        let mut public = HashSet::new();
        for stat in &parse_info.statements {
            if stat.is_public {
                public.insert(statement_name(stat));
            }
        }

        let bindings = desugar(parse_info.statements)?;
        let dir = file.parent().unwrap_or(Path::new("")).to_path_buf();

        let idx = self.modules.len();
        let prefix = if is_entry {
            String::new()
        } else {
            format!("m{idx}::")
        };

        self.modules.push(Module {
            dir,
            package,
            prefix,
            bindings,
            public,
            imports: parse_info.imports,
            exports_ast: parse_info.exports,
            resolved_imports: Vec::new(),
            resolved_exports: Vec::new(),
            own: HashMap::new(),
            base_names: HashMap::new(),
            names: HashMap::new(),
            module_aliases: HashMap::new(),
            exports: HashMap::new(),
        });
        self.path_to_module.insert(file, idx);
        Ok(idx)
    }

    fn resolve_module_path(
        &mut self,
        current: usize,
        path: &ModulePath,
    ) -> Result<usize, Diagnostics> {
        let module_package = self.modules[current].package;
        let module_dir = self.modules[current].dir.clone();

        let (base_dir, target_package, segments): (PathBuf, usize, &[_]) = match &path.anchor {
            PathAnchor::Pkg => (
                self.packages[module_package].root.clone(),
                module_package,
                &path.segments[..],
            ),
            PathAnchor::Super(count) => {
                let mut dir = module_dir.clone();
                for _ in 0..*count {
                    dir = dir.parent().unwrap_or(Path::new("")).to_path_buf();
                }
                (dir, module_package, &path.segments[..])
            }
            PathAnchor::SelfMod => (module_dir.clone(), module_package, &path.segments[..]),
            PathAnchor::Plain => {
                if let Some(head) = path.segments.first() {
                    if let Some(extern_root) =
                        self.packages[module_package].externs.get(&head.kind).cloned()
                    {
                        let pkg = self.ensure_package(&extern_root)?;
                        (self.packages[pkg].root.clone(), pkg, &path.segments[1..])
                    } else {
                        (module_dir.clone(), module_package, &path.segments[..])
                    }
                } else {
                    (module_dir.clone(), module_package, &path.segments[..])
                }
            }
        };

        if segments.is_empty() {
            return Err(Diagnostics::compiler_error(
                "module path has no module segment".to_string(),
                path.span.clone(),
            ));
        }

        let mut file = base_dir;
        for segment in segments {
            file.push(segment.kind.as_ref());
        }
        file.set_extension("par");

        self.load_module(file, target_package, false)
    }

    fn load_closure(&mut self) -> Result<(), Diagnostics> {
        let mut index = 0;
        while index < self.modules.len() {
            let import_paths: Vec<ModulePath> = self.modules[index]
                .imports
                .iter()
                .map(|import| import.path.clone())
                .collect();
            let export_paths: Vec<ModulePath> = self.modules[index]
                .exports_ast
                .iter()
                .filter_map(|export| match &export.item {
                    ExportItem::FromGlob(path) => Some(path.clone()),
                    ExportItem::FromItems(path, _) => Some(path.clone()),
                    ExportItem::Local(_) | ExportItem::LocalGlob => None,
                })
                .collect();

            for path in import_paths.iter().chain(export_paths.iter()) {
                self.resolve_module_path(index, path)?;
            }

            index += 1;
        }
        Ok(())
    }

    fn compute_own(&mut self) {
        for module in &mut self.modules {
            for binding in &module.bindings {
                let canonical: Rc<str> = Rc::from(format!("{}{}", module.prefix, binding.name));
                module.own.insert(binding.name.clone(), canonical);
            }
        }
    }

    fn resolve_specs(&mut self, prelude_names: &[Rc<str>]) -> Result<(), Diagnostics> {
        for index in 0..self.modules.len() {
            let imports_ast: Vec<parlance_parser::Import> =
                std::mem::take(&mut self.modules[index].imports);
            let exports_ast: Vec<Export> = std::mem::take(&mut self.modules[index].exports_ast);

            let mut resolved_imports = Vec::new();
            let mut module_aliases = HashMap::new();

            for import in &imports_ast {
                let target = self.resolve_module_path(index, &import.path)?;
                match &import.target {
                    ImportTarget::Module => {
                        let alias = import
                            .alias
                            .as_ref()
                            .map(|node| node.kind.clone())
                            .or_else(|| import.path.segments.last().map(|node| node.kind.clone()));
                        let Some(alias) = alias else {
                            return Err(Diagnostics::compiler_error(
                                "import path has no module segment".to_string(),
                                import.span.clone(),
                            ));
                        };
                        module_aliases.insert(alias, target);
                    }
                    ImportTarget::Glob => {
                        resolved_imports.push(ResolvedImport::Glob { target });
                    }
                    ImportTarget::Items(items) => {
                        resolved_imports.push(ResolvedImport::Items {
                            target,
                            items: items.clone(),
                        });
                    }
                }
            }

            let mut resolved_exports = Vec::new();
            for export in &exports_ast {
                match &export.item {
                    ExportItem::Local(items) => {
                        resolved_exports.push(ResolvedExport::Local(items.clone()));
                    }
                    ExportItem::LocalGlob => {
                        resolved_exports.push(ResolvedExport::LocalGlob);
                    }
                    ExportItem::FromGlob(path) => {
                        let target = self.resolve_module_path(index, path)?;
                        resolved_exports.push(ResolvedExport::FromGlob { target });
                    }
                    ExportItem::FromItems(path, items) => {
                        let target = self.resolve_module_path(index, path)?;
                        resolved_exports.push(ResolvedExport::FromItems {
                            target,
                            items: items.clone(),
                        });
                    }
                }
            }

            let mut base_names = self.modules[index].own.clone();
            for name in prelude_names {
                base_names
                    .entry(name.clone())
                    .or_insert_with(|| name.clone());
            }

            self.modules[index].resolved_imports = resolved_imports;
            self.modules[index].resolved_exports = resolved_exports;
            self.modules[index].module_aliases = module_aliases;
            self.modules[index].base_names = base_names;
        }
        Ok(())
    }

    fn compute_scopes(&mut self) {
        loop {
            let mut changed = false;

            for index in 0..self.modules.len() {
                let mut names = self.modules[index].base_names.clone();
                let mut exports = HashMap::new();

                for name in &self.modules[index].public {
                    if let Some(canonical) = self.modules[index].own.get(name) {
                        exports.insert(name.clone(), canonical.clone());
                    }
                }

                for import in &self.modules[index].resolved_imports {
                    match import {
                        ResolvedImport::Glob { target } => {
                            for (name, canonical) in &self.modules[*target].exports {
                                names.entry(name.clone()).or_insert_with(|| canonical.clone());
                            }
                        }
                        ResolvedImport::Items { target, items } => {
                            for item in items {
                                if let Some(canonical) =
                                    self.modules[*target].exports.get(&item.name.kind)
                                {
                                    names
                                        .entry(local_name(item))
                                        .or_insert_with(|| canonical.clone());
                                }
                            }
                        }
                    }
                }

                for export in &self.modules[index].resolved_exports {
                    match export {
                        ResolvedExport::FromGlob { target } => {
                            for (name, canonical) in &self.modules[*target].exports {
                                names.entry(name.clone()).or_insert_with(|| canonical.clone());
                                exports.insert(name.clone(), canonical.clone());
                            }
                        }
                        ResolvedExport::FromItems { target, items } => {
                            for item in items {
                                if let Some(canonical) =
                                    self.modules[*target].exports.get(&item.name.kind)
                                {
                                    let key = local_name(item);
                                    names
                                        .entry(key.clone())
                                        .or_insert_with(|| canonical.clone());
                                    exports.insert(key, canonical.clone());
                                }
                            }
                        }
                        ResolvedExport::Local(_) | ResolvedExport::LocalGlob => {}
                    }
                }

                for export in &self.modules[index].resolved_exports {
                    match export {
                        ResolvedExport::Local(items) => {
                            for item in items {
                                if let Some(canonical) = names.get(&item.name.kind) {
                                    exports.insert(local_name(item), canonical.clone());
                                }
                            }
                        }
                        ResolvedExport::LocalGlob => {
                            for (name, canonical) in &names {
                                exports.insert(name.clone(), canonical.clone());
                            }
                        }
                        _ => {}
                    }
                }

                if names != self.modules[index].names {
                    self.modules[index].names = names;
                    changed = true;
                }
                if exports != self.modules[index].exports {
                    self.modules[index].exports = exports;
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }
    }

    fn validate(&self) -> Result<(), Diagnostics> {
        for module in &self.modules {
            for import in &module.resolved_imports {
                if let ResolvedImport::Items { target, items } = import {
                    for item in items {
                        if !self.modules[*target].exports.contains_key(&item.name.kind) {
                            return Err(Diagnostics::compiler_error(
                                format!(
                                    "'{}' is not exported by the imported module",
                                    item.name.kind
                                ),
                                item.name.span.clone(),
                            ));
                        }
                    }
                }
            }

            for export in &module.resolved_exports {
                match export {
                    ResolvedExport::Local(items) => {
                        for item in items {
                            if !module.names.contains_key(&item.name.kind) {
                                return Err(Diagnostics::compiler_error(
                                    format!("export of unresolved name '{}'", item.name.kind),
                                    item.name.span.clone(),
                                ));
                            }
                        }
                    }
                    ResolvedExport::FromItems { target, items } => {
                        for item in items {
                            if !self.modules[*target].exports.contains_key(&item.name.kind) {
                                return Err(Diagnostics::compiler_error(
                                    format!(
                                        "re-export of '{}' which is not exported",
                                        item.name.kind
                                    ),
                                    item.name.span.clone(),
                                ));
                            }
                        }
                    }
                    ResolvedExport::LocalGlob | ResolvedExport::FromGlob { .. } => {}
                }
            }
        }
        Ok(())
    }

    fn resolve_ref(
        &self,
        module: usize,
        name: &Rc<str>,
        span: &Span,
    ) -> Result<Rc<str>, Diagnostics> {
        if let Some((head, rest)) = name.split_once("::") {
            if let Some(&target) = self.modules[module].module_aliases.get(head) {
                if let Some(canonical) = self.modules[target].exports.get(rest) {
                    return Ok(canonical.clone());
                }
                return Err(Diagnostics::compiler_error(
                    format!("'{rest}' is not exported by module '{head}'"),
                    span.clone(),
                ));
            }
        }

        if let Some(canonical) = self.modules[module].names.get(name) {
            return Ok(canonical.clone());
        }

        Err(Diagnostics::compiler_error(
            format!("unresolved name '{name}'"),
            span.clone(),
        ))
    }

    fn canonicalize_value(
        &self,
        module: usize,
        value: &Rc<DesugarValue>,
        bound: &HashSet<Rc<str>>,
    ) -> Result<Rc<DesugarValue>, Diagnostics> {
        let kind = match &value.kind {
            DesugarValueKind::Variable { name } => {
                if !name.contains("::") && bound.contains(name) {
                    return Ok(value.clone());
                }
                DesugarValueKind::Variable {
                    name: self.resolve_ref(module, name, &value.span)?,
                }
            }
            DesugarValueKind::Function { param, body } => {
                let mut inner = bound.clone();
                inner.insert(param.name.clone());
                DesugarValueKind::Function {
                    param: param.clone(),
                    body: self.canonicalize_value(module, body, &inner)?,
                }
            }
            DesugarValueKind::FunctionCall { callee, arg } => DesugarValueKind::FunctionCall {
                callee: self.canonicalize_value(module, callee, bound)?,
                arg: self.canonicalize_value(module, arg, bound)?,
            },
            DesugarValueKind::String(text) => DesugarValueKind::String(text.clone()),
            DesugarValueKind::Int(int) => DesugarValueKind::Int(*int),
        };

        Ok(Rc::new(DesugarValue {
            span: value.span.clone(),
            kind,
        }))
    }

    fn canonicalize_binding(
        &self,
        module: usize,
        binding: &DesugarBinding,
        outer_bound: &HashSet<Rc<str>>,
        rename: bool,
    ) -> Result<DesugarBinding, Diagnostics> {
        let mut bound = outer_bound.clone();
        for scheme_binding in &binding.scheme {
            bound.insert(scheme_binding.name.clone());
        }

        let mut scheme = Vec::with_capacity(binding.scheme.len());
        for scheme_binding in &binding.scheme {
            scheme.push(self.canonicalize_binding(module, scheme_binding, &bound, false)?);
        }

        let value = self.canonicalize_value(module, &binding.value, &bound)?;

        let name = if rename {
            self.modules[module]
                .own
                .get(&binding.name)
                .cloned()
                .unwrap_or_else(|| binding.name.clone())
        } else {
            binding.name.clone()
        };

        Ok(DesugarBinding {
            name,
            value,
            scheme,
        })
    }

    fn canonicalize_all(&self) -> Result<Vec<DesugarBinding>, Diagnostics> {
        let empty = HashSet::new();
        let mut output = Vec::new();

        for index in 0..self.modules.len() {
            for binding in &self.modules[index].bindings {
                output.push(self.canonicalize_binding(index, binding, &empty, true)?);
            }
        }

        Ok(output)
    }
}

pub fn resolve_program(
    entry: &Path,
    injected_externs: HashMap<Rc<str>, PathBuf>,
    prelude_names: &[Rc<str>],
) -> Result<Vec<DesugarBinding>, Diagnostics> {
    resolve_program_with_source(&FsModuleSource, entry, injected_externs, prelude_names)
}

pub fn resolve_program_with_source(
    source: &dyn ModuleSource,
    entry: &Path,
    injected_externs: HashMap<Rc<str>, PathBuf>,
    prelude_names: &[Rc<str>],
) -> Result<Vec<DesugarBinding>, Diagnostics> {
    let mut resolver = Resolver::new(source, injected_externs);

    let entry_dir = entry.parent().unwrap_or(Path::new("."));
    let root = resolver
        .find_manifest_root(entry_dir)
        .unwrap_or_else(|| entry_dir.to_path_buf());
    let package = resolver.ensure_package(&root)?;

    resolver.load_module(entry.to_path_buf(), package, true)?;
    resolver.load_closure()?;

    resolver.compute_own();
    resolver.resolve_specs(prelude_names)?;
    resolver.compute_scopes();
    resolver.validate()?;

    resolver.canonicalize_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct MemorySource {
        files: HashMap<PathBuf, String>,
        reads: RefCell<usize>,
    }

    impl MemorySource {
        fn with(files: &[(&str, &str)]) -> Self {
            Self {
                files: files
                    .iter()
                    .map(|(path, body)| (PathBuf::from(path), body.to_string()))
                    .collect(),
                reads: RefCell::new(0),
            }
        }
    }

    impl ModuleSource for MemorySource {
        fn canonicalize(&self, path: &Path) -> PathBuf {
            path.to_path_buf()
        }

        fn read_to_string(&self, path: &Path) -> Option<String> {
            *self.reads.borrow_mut() += 1;
            self.files.get(path).cloned()
        }

        fn is_file(&self, path: &Path) -> bool {
            self.files.contains_key(path)
        }
    }

    fn names(bindings: &[DesugarBinding]) -> Vec<String> {
        bindings.iter().map(|b| b.name.to_string()).collect()
    }

    #[test]
    fn resolves_in_memory_without_touching_disk() {
        let source = MemorySource::with(&[
            ("/app/parlance.toml", "[package]\nname = \"app\"\n"),
            ("/app/main.par", "import pkg::util::io::{answer}\nmain = answer\n"),
            ("/app/util/io.par", "public answer = 42\n"),
        ]);

        let bindings = resolve_program_with_source(
            &source,
            Path::new("/app/main.par"),
            HashMap::new(),
            &[],
        )
        .expect("resolution should succeed purely in memory");

        let names = names(&bindings);
        assert!(names.iter().any(|n| n == "main"), "entry keeps bare name");
        assert!(
            names.iter().any(|n| n.ends_with("::answer")),
            "imported binding gets a canonical module-prefixed name: {names:?}"
        );
        assert_eq!(bindings.len(), 2);
        assert!(*source.reads.borrow() >= 2, "the trait drove all file reads");
    }

    #[test]
    fn export_star_reexports_imported_symbols() {
        let source = MemorySource::with(&[
            ("/app/parlance.toml", "[package]\nname = \"app\"\n"),
            (
                "/app/main.par",
                "import pkg::bridge::*\nmain = answer\n",
            ),
            (
                "/app/bridge.par",
                "import pkg::leaf::{answer}\nexport *\n",
            ),
            ("/app/leaf.par", "public answer = 7\n"),
        ]);

        let bindings = resolve_program_with_source(
            &source,
            Path::new("/app/main.par"),
            HashMap::new(),
            &[],
        )
        .expect("export * should re-export imported symbols");

        assert!(names(&bindings).iter().any(|n| n == "main"));
    }

    #[test]
    fn item_alias_renames_and_hides_source_name() {
        let source = MemorySource::with(&[
            ("/app/parlance.toml", "[package]\nname = \"app\"\n"),
            (
                "/app/main.par",
                "import pkg::io::{answer as a}\nmain = a\n",
            ),
            ("/app/io.par", "public answer = 9\n"),
        ]);

        let ok = resolve_program_with_source(
            &source,
            Path::new("/app/main.par"),
            HashMap::new(),
            &[],
        );
        assert!(ok.is_ok(), "aliased import binds the alias name");

        let hidden = MemorySource::with(&[
            ("/app/parlance.toml", "[package]\nname = \"app\"\n"),
            (
                "/app/main.par",
                "import pkg::io::{answer as a}\nmain = answer\n",
            ),
            ("/app/io.par", "public answer = 9\n"),
        ]);
        let err = resolve_program_with_source(
            &hidden,
            Path::new("/app/main.par"),
            HashMap::new(),
            &[],
        );
        assert!(err.is_err(), "the original source name must not be bound");
    }

    #[test]
    fn private_symbol_is_not_importable() {
        let source = MemorySource::with(&[
            ("/app/parlance.toml", "[package]\nname = \"app\"\n"),
            ("/app/main.par", "import pkg::io::{secret}\nmain = secret\n"),
            ("/app/io.par", "secret = 1\n"),
        ]);

        let result = resolve_program_with_source(
            &source,
            Path::new("/app/main.par"),
            HashMap::new(),
            &[],
        );

        assert!(result.is_err(), "non-public symbol must not be importable");
    }
}
