use std::fs;
use std::path::Path;

use failure::{format_err, Context, Error, ResultExt};

use crate::build_context::BridgeModel;
use crate::compile::compile;
use crate::module_writer::write_bindings_module;
use crate::module_writer::write_cffi_module;
use crate::module_writer::DevelopModuleWriter;
use crate::BuildOptions;
use crate::Manylinux;
use crate::PythonInterpreter;
use crate::Target;

/// Installs a crate by compiling it and copying the shared library to the right directory
///
/// Works only in virtualenvs.
pub fn develop(
    bindings: Option<String>,
    manifest_file: &Path,
    cargo_extra_args: Vec<String>,
    rustc_extra_args: Vec<String>,
    venv_dir: &Path,
    release: bool,
    strip: bool,
    python_feature_gate: Option<String>,
) -> Result<(), Error> {
    let target = Target::from_target_triple(None)?;

    let python = target.get_venv_python(&venv_dir);

    let interpreter = PythonInterpreter::check_executable(python, &target)?.ok_or_else(|| {
        Context::new("Expected `python` to be a python interpreter inside a virtualenv ಠ_ಠ")
    })?;

    let build_options = BuildOptions {
        manylinux: Manylinux::Off,
        interpreter: vec!["python".to_string()],
        bindings,
        manifest_path: manifest_file.to_path_buf(),
        out: None,
        skip_auditwheel: false,
        target: None,
        cargo_extra_args,
        rustc_extra_args,
        python_feature_gate,
    };

    let build_context = build_options.into_build_context(release, strip)?;

    let mut builder = DevelopModuleWriter::venv(&target, &venv_dir)?;

    let context = "Failed to build a native library through cargo";

    match build_context.bridge {
        BridgeModel::Bin => {
            let artifacts = compile(&build_context, None, &BridgeModel::Bin).context(context)?;

            let artifact = artifacts
                .get("bin")
                .ok_or_else(|| format_err!("Cargo didn't build a binary"))?;

            // Copy the artifact into the same folder as pip and python
            let bin_name = artifact.file_name().unwrap();
            let bin_path = target.get_venv_bin_dir(&venv_dir).join(bin_name);
            fs::copy(artifact, bin_path)?;
        }
        BridgeModel::Cffi => {
            let artifact = build_context.compile_cdylib(None, None).context(context)?;

            write_cffi_module(
                &mut builder,
                &build_context.manifest_path.parent().unwrap(),
                &build_context.module_name,
                &artifact,
                &interpreter.executable,
            )?;
        }
        BridgeModel::Bindings(_) => {
            let artifact = build_context
                .compile_cdylib(Some(&interpreter), Some(&build_context.module_name))
                .context(context)?;

            write_bindings_module(
                &mut builder,
                &build_context.module_name,
                &artifact,
                &interpreter,
            )?;
        }
    }

    Ok(())
}
