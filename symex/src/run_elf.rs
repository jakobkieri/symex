//! Simple runner that starts symbolic execution on LLVM bitcode.
use std::{fs, path::Path, time::Instant};

use object::{Architecture, Object};
use regex::Regex;
use tracing::{debug, trace};

use crate::{
    elf_util::{ErrorReason, PathStatus, VisualPathResult},
    general_assembly::{
        self,
        arch::{
            arm::{v6::ArmV6M, v7::ArmV7EM},
            Arch,
        },
        executor::PathResult,
        project::{PCHook, ProjectError},
        state::GAState,
        GAError,
        RunConfig,
    },
    smt::DContext,
};

fn add_architecture_independent_hooks<A: Arch>(cfg: &mut RunConfig<A>) {
    // intrinsic functions
    let start_cyclecount = |state: &mut GAState<A>| {
        state.cycle_count = 0;
        trace!("Reset the cycle count (cycle count: {})", state.cycle_count);

        // jump back to where the function was called from
        let lr = state.get_register("LR".to_owned()).unwrap();
        state.set_register("PC".to_owned(), lr)?;
        Ok(())
    };
    let end_cyclecount = |state: &mut GAState<A>| {
        // stop counting
        state.count_cycles = false;
        trace!(
            "Stopped counting cycles (cycle count: {})",
            state.cycle_count
        );

        // jump back to where the function was called from
        let lr = state.get_register("LR".to_owned()).unwrap();
        state.set_register("PC".to_owned(), lr)?;
        Ok(())
    };

    // add all pc hooks
    cfg.pc_hooks.extend([
        (
            Regex::new(r"^panic_cold_explicit$").unwrap(),
            PCHook::EndFailure("explicit panic"),
        ),
        (
            Regex::new("^unwrap_failed$").unwrap(),
            PCHook::EndFailure("unwrap failed"),
        ),
        (
            Regex::new(r"^panic_bounds_check$").unwrap(),
            PCHook::EndFailure("bounds check panic"),
        ),
        (Regex::new(r"^suppress_path$").unwrap(), PCHook::Suppress),
        (
            Regex::new(r"^unreachable_unchecked$").unwrap(),
            PCHook::EndFailure("reach a unreachable unchecked call undefined behavior"),
        ),
        (
            Regex::new(r"^start_cyclecount$").unwrap(),
            PCHook::Intrinsic(start_cyclecount),
        ),
        (
            Regex::new(r"^end_cyclecount$").unwrap(),
            PCHook::Intrinsic(end_cyclecount),
        ),
        (
            Regex::new(r"^panic_*").unwrap(),
            PCHook::EndFailure("panic"),
        ),
    ]);
}

/// Run symbolic execution on a elf file where `path` is the path to the ELF
/// file and `function` is the function the execution starts at.
/// During runtime it will determin the target architecture and select the
/// appropriate executor for that enviornement.
pub fn run_elf<P: AsRef<Path>>(
    path: P,
    function: &str,
    show_path_results: bool,
) -> Result<Vec<VisualPathResult>, GAError> {
    let context = Box::new(DContext::new());
    let context = Box::leak(context);

    let end_pc = 0xFFFFFFFE;

    let str_version = path.as_ref().display().to_string();
    debug!("Parsing elf file: {}", str_version);
    let file = fs::read(path).expect("Unable to open file.");
    let data = file.as_ref();
    let obj_file = match object::File::parse(data) {
        Ok(x) => x,
        Err(e) => {
            debug!("Error: {}", e);
            return Err(ProjectError::UnableToParseElf(str_version))?;
        }
    };

    let architecture = obj_file.architecture();

    match architecture {
        Architecture::Arm => {
            // Run the paths with architecture specific data.
            if let Some(v7) = ArmV7EM::discover(&obj_file)? {
                let mut cfg = RunConfig {
                    show_path_results,
                    pc_hooks: Vec::new(),
                    register_read_hooks: Vec::new(),
                    register_write_hooks: Vec::new(),
                    memory_write_hooks: Vec::new(),
                    memory_read_hooks: Vec::new(),
                };

                cfg.show_path_results = show_path_results;

                add_architecture_independent_hooks(&mut cfg);
                let project = Box::new(general_assembly::project::Project::from_path(
                    &mut cfg, obj_file, &v7,
                )?);
                let project = Box::leak(project);
                project.add_pc_hook(end_pc, PCHook::EndSuccess);
                debug!("Created project: {:?}", project);

                let mut vm = general_assembly::vm::VM::new(project, context, function, end_pc, v7)?;

                return run_elf_paths(&mut vm, &cfg);
            } else if let Some(v6) = ArmV6M::discover(&obj_file)? {
                let mut cfg = RunConfig {
                    show_path_results,
                    pc_hooks: Vec::new(),
                    register_read_hooks: Vec::new(),
                    register_write_hooks: Vec::new(),
                    memory_write_hooks: Vec::new(),
                    memory_read_hooks: Vec::new(),
                };

                add_architecture_independent_hooks(&mut cfg);
                let project = Box::new(general_assembly::project::Project::from_path(
                    &mut cfg, obj_file, &v6,
                )?);
                let project = Box::leak(project);
                project.add_pc_hook(end_pc, PCHook::EndSuccess);
                debug!("Created project: {:?}", project);

                let mut vm = general_assembly::vm::VM::new(project, context, function, end_pc, v6)?;
                return run_elf_paths(&mut vm, &cfg);
            }
        }
        _ => todo!(),
    }

    Err(ProjectError::UnableToParseElf(
        "UnsuportedArchitechture".to_string(),
    ))?
}

/// Run symbolic execution on a elf file where `path` is the path to the ELF
/// file and `function` is the function the execution starts at.
/// Execution will use the provided [`RunConfig`] and allows for pre-configured
/// hooks.
pub fn run_elf_configured<A: Arch>(
    path: &str,
    function: &str,
    architecture: A,
    mut cfg: RunConfig<A>,
) -> Result<Vec<VisualPathResult>, GAError> {
    let context = Box::new(DContext::new());
    let context = Box::leak(context);

    let end_pc = 0xFFFFFFFE;

    debug!("Parsing elf file: {}", path);
    let file = fs::read(path).expect("Unable to open file.");
    let data = file.as_ref();
    let obj_file = match object::File::parse(data) {
        Ok(x) => x,
        Err(e) => {
            debug!("Error: {}", e);
            return Err(ProjectError::UnableToParseElf(path.to_owned()))?;
        }
    };

    add_architecture_independent_hooks(&mut cfg);
    let project = Box::new(general_assembly::project::Project::from_path(
        &mut cfg,
        obj_file,
        &architecture,
    )?);
    let project = Box::leak(project);
    project.add_pc_hook(end_pc, PCHook::EndSuccess);
    debug!("Created project: {:?}", project);

    let mut vm = general_assembly::vm::VM::new(project, context, function, end_pc, architecture)?;
    run_elf_paths(&mut vm, &cfg)
}

/// Runs all paths in the vm
fn run_elf_paths<A: Arch>(
    vm: &mut general_assembly::vm::VM<A>,
    cfg: &RunConfig<A>,
) -> Result<Vec<VisualPathResult>, GAError> {
    let mut path_num = 0;
    let start = Instant::now();
    let mut path_results = vec![];
    while let Some((path_result, state)) = vm.run()? {
        if matches!(path_result, PathResult::Suppress) {
            debug!("Suppressing path");
            continue;
        }
        if matches!(path_result, PathResult::AssumptionUnsat) {
            println!("Encountered an unsatisfiable assumption, ignoring this path");
            continue;
        }

        path_num += 1;

        let v_path_result = match path_result {
            general_assembly::executor::PathResult::Success(_v) => PathStatus::Ok(None),
            general_assembly::executor::PathResult::Failure(reason) => {
                PathStatus::Failed(ErrorReason {
                    error_message: reason.to_owned(),
                })
            }
            general_assembly::executor::PathResult::AssumptionUnsat => todo!(),
            general_assembly::executor::PathResult::Suppress => todo!(),
        };

        let result = VisualPathResult::from_state(state, path_num, v_path_result)?;

        if cfg.show_path_results {
            println!("{}", result);
        }
        path_results.push(result);
    }
    if cfg.show_path_results {
        println!("time: {:?}", start.elapsed());
    }
    Ok(path_results)
}
