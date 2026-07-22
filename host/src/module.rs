use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleManifest {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ModuleKind,
    pub entry: String,
    #[serde(default = "default_workgroup")]
    pub workgroup: [u32; 3],
    #[serde(default)]
    pub state: ModuleState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleKind {
    Compute,
    Fragment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleState {
    #[default]
    None,
    /// RGBA32F buffers for simulation state (e.g. reaction–diffusion).
    PingPong,
    /// RGBA32F ping-pong buffers; texel (0,0) stores integrator state for trail modules.
    Trail,
}

#[derive(Debug, Clone)]
pub struct LoadedModule {
    pub manifest: ModuleManifest,
    pub dir: PathBuf,
}

fn default_workgroup() -> [u32; 3] {
    [16, 16, 1]
}

impl LoadedModule {
    pub fn shader_path(&self) -> PathBuf {
        self.dir.join(&self.manifest.entry)
    }

    pub fn read_shader_source(&self) -> Result<String, String> {
        fs::read_to_string(self.shader_path())
            .map_err(|err| format!("failed to read {}: {err}", self.shader_path().display()))
    }
}

pub fn discover_modules(path: &Path) -> Result<Vec<LoadedModule>, String> {
    if !path.is_dir() {
        return Err(format!("modules path is not a directory: {}", path.display()));
    }

    let mut modules = Vec::new();

    for entry in fs::read_dir(path).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }

        let manifest_path = dir.join("module.toml");
        if !manifest_path.is_file() {
            continue;
        }

        let manifest_text =
            fs::read_to_string(&manifest_path).map_err(|err| err.to_string())?;
        let manifest: ModuleManifest =
            toml::from_str(&manifest_text).map_err(|err| format!("{}: {err}", manifest_path.display()))?;

        modules.push(LoadedModule { manifest, dir });
    }

    modules.sort_by(|a, b| a.dir.file_name().cmp(&b.dir.file_name()));
    if modules.is_empty() {
        return Err(format!("no modules found in {}", path.display()));
    }

    Ok(modules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_fields() {
        let manifest: ModuleManifest = toml::from_str(
            r#"
            name = "Test"
            type = "compute"
            entry = "main.comp"
            workgroup = [8, 8, 1]
            state = "ping-pong"
            "#,
        )
        .unwrap();

        assert_eq!(manifest.name, "Test");
        assert_eq!(manifest.kind, ModuleKind::Compute);
        assert_eq!(manifest.entry, "main.comp");
        assert_eq!(manifest.workgroup, [8, 8, 1]);
        assert_eq!(manifest.state, ModuleState::PingPong);
    }
}
