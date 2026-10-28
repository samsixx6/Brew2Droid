//! Bancada de handset: registra onde o candidato de runtime para, sem declarar compatibilidade.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::handset::input::{Key, Keypad};
use zeebx::handset::package::{parse_module, read_package};
use zeebx::handset::types::DisplayGeometry;
use zeebx::loader::{self, miffile::MifFile};
use zeebx::machine::{AppletResult, Machine, Outcome};
use zeebx::storage::StoragePaths;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn checked_private_directory(path: &Path) -> Result<PathBuf> {
    #[cfg(not(target_os = "android"))]
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    #[cfg(target_os = "android")]
    let workspace = {
        let expected = Path::new("/data/local/tmp/brew-handset-probe");
        let actual = expected.canonicalize()?;
        if actual != expected {
            return Err("Android probe root must not redirect to another directory".into());
        }
        actual
    };
    #[cfg(not(target_os = "android"))]
    let allowed = workspace.join("local-tests");
    #[cfg(target_os = "android")]
    let allowed = workspace.join("runs");
    std::fs::create_dir_all(&allowed)?;
    let expected = allowed;
    let allowed = expected.canonicalize()?;
    if allowed != expected {
        return Err("Private probe run root must not redirect to another directory".into());
    }
    if !allowed.starts_with(&workspace) {
        return Err("Private test directory escapes the workspace".into());
    }
    let actual = path.canonicalize()?;
    if !actual.starts_with(allowed) {
        return Err("Work/output directories must be inside the private probe root".into());
    }
    Ok(actual)
}

fn copy_content(source: &Path, destination: &Path) -> Result<()> {
    std::fs::create_dir(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err("Linked content is not supported by the private probe".into());
        }
        let output = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_content(&entry.path(), &output)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), output)?;
        }
    }
    Ok(())
}

fn outcome(value: &Outcome) -> Value {
    match value {
        Outcome::Returned { code } => json!({"kind":"returned", "code":code}),
        Outcome::Fault { addr, pc, lr } => {
            json!({"kind":"memory_fault", "address":addr, "pc":pc, "lr":lr})
        }
        Outcome::Exception { pc } => json!({"kind":"cpu_exception", "pc":pc}),
        Outcome::Budget => json!({"kind":"instruction_budget"}),
        Outcome::CallLimit { calls } => json!({"kind":"api_call_limit", "calls":calls}),
        Outcome::Unimplemented { addr, args, caller } => {
            let decoded = zeebx::brew::aee::decode(*addr)
                .map(|(interface, slot)| json!({"interface":interface.name(), "slot":slot}));
            json!({"kind":"unsupported_api", "address":addr, "arguments":args, "caller":caller, "decoded":decoded})
        }
    }
}

fn synthetic_module() -> Vec<u8> {
    // O primeiro helper de Bowling observado é MALLOC no offset 0x68. O teste atravessa o
    // mesmo trampolim e escreve a saída de AEEMod_Load com alinhamento de pilha preservado.
    [
        0xe92d_4010u32,
        0xe1a0_4002,
        0xe591_3068,
        0xe3a0_0020,
        0xe12f_ff33,
        0xe584_0000,
        0xe3a0_0000,
        0xe8bd_8010,
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect()
}

fn finish_bounded_call(
    machine: &mut Machine<BackendPadrao>,
    mut value: Outcome,
    budget: u64,
    resumptions: &mut u32,
) -> Result<Outcome> {
    for _ in 0..8 {
        if value != Outcome::Budget {
            break;
        }
        // advance retoma o trecho suspenso antes de entregar timers. Não chama a função de novo.
        let mut outcomes = machine.advance(budget)?;
        if outcomes.len() != 1 {
            return Err("Suspended startup call has no unique continuation".into());
        }
        value = outcomes.remove(0);
        *resumptions += 1;
    }
    Ok(value)
}

fn callback_snapshot(
    machine: &Machine<BackendPadrao>,
    outcomes: &[Outcome],
    step: u32,
    resumed: u32,
) -> Value {
    json!({
        "step":step, "resumptions":resumed,
        "outcomes":outcomes.iter().map(outcome).collect::<Vec<_>>(),
        "pc":machine.cpu().read_reg(Reg::Pc), "lr":machine.cpu().read_reg(Reg::Lr),
        "cpsr":machine.cpu().cpsr(), "instructions":machine.cpu().instructions(),
        "api_calls":machine.api_calls(), "guest_clock_ms":machine.clock_ms(),
    })
}

struct KeyAction {
    step: u32,
    key: Key,
    down: bool,
}

fn read_key_script(path: &Path, steps: u32) -> Result<Vec<KeyAction>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(65_537)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 65_536 {
        return Err("Handset key script exceeds 64 KiB".into());
    }
    let value: Value = serde_json::from_slice(&bytes)?;
    let actions = value
        .as_array()
        .ok_or("Handset key script must be an array")?;
    if actions.len() > 128 {
        return Err("Handset key script exceeds 128 actions".into());
    }
    let mut parsed = Vec::new();
    let mut previous = 0;
    for action in actions {
        let step = action["step"]
            .as_u64()
            .ok_or("Handset key step is missing")?;
        if step >= u64::from(steps) || step < previous {
            return Err("Handset key steps must be ordered and inside the advance window".into());
        }
        let key = action["key"]
            .as_str()
            .and_then(Key::from_name)
            .ok_or("Unknown handset key")?;
        let down = action["down"]
            .as_bool()
            .ok_or("Handset key down must be boolean")?;
        parsed.push(KeyAction {
            step: step as u32,
            key,
            down,
        });
        previous = step;
    }
    Ok(parsed)
}

fn complete_callback(
    machine: &mut Machine<BackendPadrao>,
    mut outcomes: Vec<Outcome>,
    step: u32,
    budget: u64,
    limit: u32,
    resumed: &mut u32,
    slices: &mut Vec<Value>,
) -> Result<Vec<Outcome>> {
    if matches!(outcomes.as_slice(), [Outcome::Budget]) {
        slices.push(callback_snapshot(machine, &outcomes, step, *resumed));
        // O limite cobre a execução inteira; uma continuação ambígua não é retomada.
        while matches!(outcomes.as_slice(), [Outcome::Budget]) && *resumed < limit {
            outcomes = machine.advance(budget)?;
            *resumed += 1;
            slices.push(callback_snapshot(machine, &outcomes, step, *resumed));
        }
    }
    Ok(outcomes)
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().ok_or(
        "Usage: brew-probe synthetic|inspect|trace-start [module mif] --work-dir DIR --output JSON",
    )?;
    if !["synthetic", "inspect", "trace-start"].contains(&command.as_str()) {
        return Err("Unknown probe command".into());
    }
    let option = |name: &str| -> Result<&str> {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|index| args.get(index + 1))
            .map(String::as_str)
            .ok_or_else(|| format!("Missing {name}").into())
    };
    let budget = if args.iter().any(|arg| arg == "--budget") {
        option("--budget")?.parse::<u64>()?
    } else {
        100_000
    };
    if !(1..=10_000_000).contains(&budget) {
        return Err("Instruction budget must be between 1 and 10000000".into());
    }
    let advance_steps = if args.iter().any(|arg| arg == "--advance") {
        option("--advance")?.parse::<u32>()?
    } else {
        0
    };
    if advance_steps > 3600 {
        return Err("Advance limit must be at most 3600".into());
    }
    let callback_resume_limit = if args.iter().any(|arg| arg == "--callback-resumes") {
        option("--callback-resumes")?.parse::<u32>()?
    } else {
        0
    };
    if callback_resume_limit > 32 {
        return Err("Callback resume limit must be at most 32".into());
    }
    let key_actions = if args.iter().any(|arg| arg == "--key-script") {
        if command != "trace-start" {
            return Err("Handset input requires trace-start".into());
        }
        read_key_script(Path::new(option("--key-script")?), advance_steps)?
    } else {
        Vec::new()
    };
    let capture = args.iter().any(|arg| arg == "--capture");
    let candidate_geometry = if args.iter().any(|arg| arg == "--candidate-display") {
        let (width, height) = option("--candidate-display")?
            .split_once('x')
            .ok_or("Candidate display must use WIDTHxHEIGHT")?;
        Some(DisplayGeometry {
            width: width.parse()?,
            height: height.parse()?,
            depth: 16,
        })
    } else {
        None
    };
    if (capture || candidate_geometry.is_some()) && command == "inspect" {
        return Err("Display diagnostics require guest execution".into());
    }
    let work = checked_private_directory(Path::new(option("--work-dir")?))?;
    let output = PathBuf::from(option("--output")?);
    let output_parent = checked_private_directory(
        output
            .parent()
            .ok_or("Output requires a parent directory")?,
    )?;
    if !output_parent.starts_with(&work) {
        return Err("Output must belong to the private work directory".into());
    }
    let output = output_parent.join(output.file_name().ok_or("Output requires a file name")?);
    if std::fs::symlink_metadata(&output).is_ok() {
        return Err("Output already exists; preserve each diagnostic run".into());
    }
    let synthetic = command == "synthetic";
    let (image, mif, source, package_report) = if synthetic {
        (parse_module(synthetic_module())?, None, None, None)
    } else {
        let module = PathBuf::from(args.get(1).ok_or("Missing module")?).canonicalize()?;
        let mif_path = PathBuf::from(args.get(2).ok_or("Missing MIF")?).canonicalize()?;
        for source_directory in [module.parent(), mif_path.parent()] {
            let source_directory = source_directory.ok_or("Source has no parent directory")?;
            if work.starts_with(source_directory)
                || source_directory.starts_with(&work)
                || output.starts_with(source_directory)
            {
                return Err("Source and private work/output trees must not overlap".into());
            }
        }
        let package = read_package(&module, &mif_path)?;
        (
            package.image,
            Some(package.mif),
            Some(module),
            Some(package.report),
        )
    };
    let mut report = json!({
        "schema_version":1, "mode":command, "source_module":source,
        "host_arch":std::env::consts::ARCH,
        "host_os":std::env::consts::OS,
        "execution_environment":if cfg!(target_os="android") {"android-adb-shell"} else {"host-process"},
        "cpu_backend":if cfg!(any(target_os="ios",target_arch="wasm32")) {"interpreter"} else {"dynarmic"},
        "instruction_budget":budget,
        "advance_steps_requested":advance_steps,
        "callback_resume_limit":callback_resume_limit,
        "key_actions_requested":key_actions.len(), "input_clock":"deterministic-guest-us",
        "candidate_geometry_requested":candidate_geometry,
        "image_bytes":image.image().len(), "entry_offset":image.entry(),
        "applet_clsid":mif.as_ref().and_then(MifFile::main_applet),
        "declared_classes":mif.as_ref().map(|m| &m.classes),
        "runtime_profile":"donor-layout-candidate", "handset_profile_verified":false,
        "gameplay_validated":false, "rendering_path":"unobserved",
        "package":package_report,
    });
    if command != "inspect" {
        let captures = work.join("captures");
        if capture {
            std::fs::create_dir(&captures)?;
        }
        let content = work.join("content");
        if let Some(module_path) = &source {
            let source_directory = module_path
                .parent()
                .ok_or("Module has no parent directory")?;
            copy_content(source_directory, &content)?;
        } else {
            std::fs::create_dir(&content)?;
        }
        let runtime = work.join("runtime");
        // create_dir recusa árvores existentes, incluindo links que redirecionariam o storage.
        std::fs::create_dir(&runtime)?;
        let storage = StoragePaths::from_root(runtime);
        storage.create_dirs()?;
        let overlay = work.join("save-overlay");
        std::fs::create_dir(&overlay)?;
        let loaded = loader::load_with(&image, &[])?;
        let cpu = BackendPadrao::new()?;
        let mut machine = match candidate_geometry {
            Some(geometry) => Machine::new_with_storage_geometry(
                cpu,
                loaded,
                content,
                &storage,
                Some(overlay),
                geometry,
            )?,
            None => Machine::new_with_storage(cpu, loaded, content, &storage, Some(overlay)),
        };
        machine.set_tracing(true);
        // O limite cobre cada entrada; o Machine também limita o total de instruções/chamadas.
        let mut resumptions = 0;
        let initial = machine.run(budget)?;
        let loaded = finish_bounded_call(&mut machine, initial, budget, &mut resumptions)?;
        // run inicializa/reset o mapa CPU; ligar antes dele perderia a opção no reset.
        if args.iter().any(|arg| arg == "--profile-memory") {
            machine.cpu_mut().liga_contagem_de_memoria(true);
        }
        report["module_load"] = outcome(&loaded);
        let module_pointer = machine.cpu().read_u32(machine.module().out_module)?;
        report["module_pointer"] = json!(module_pointer);
        if !synthetic && loaded == (Outcome::Returned { code: 0 }) {
            if let Some(clsid) = mif.as_ref().and_then(MifFile::main_applet) {
                let created = match machine.create_applet(clsid, budget) {
                    Ok(AppletResult::Stopped(value)) => {
                        let value =
                            finish_bounded_call(&mut machine, value, budget, &mut resumptions)?;
                        Ok(match value {
                            Outcome::Returned { code } => AppletResult::Called {
                                code,
                                applet: machine.cpu().read_u32(machine.module().out_module + 4)?,
                            },
                            other => AppletResult::Stopped(other),
                        })
                    }
                    other => other,
                };
                match created {
                    Ok(AppletResult::Called { code, applet }) => {
                        report["create_applet"] =
                            json!({"kind":"returned", "code":code, "applet":applet});
                        if code == 0 && applet != 0 {
                            report["app_start"] = match machine.start_applet(applet, clsid, budget)
                            {
                                Ok(value) => outcome(&finish_bounded_call(
                                    &mut machine,
                                    value,
                                    budget,
                                    &mut resumptions,
                                )?),
                                Err(error) => {
                                    json!({"kind":"adapter_error", "message":error.to_string()})
                                }
                            };
                        }
                    }
                    Ok(AppletResult::Stopped(value)) => report["create_applet"] = outcome(&value),
                    Ok(AppletResult::NoModule) => {
                        report["create_applet"] = json!({"kind":"no_module"})
                    }
                    Err(error) => {
                        report["create_applet"] =
                            json!({"kind":"adapter_error", "message":error.to_string()})
                    }
                }
            }
        }
        let started = report["app_start"]["kind"] == "returned" && report["app_start"]["code"] == 1;
        let mut steps = Vec::new();
        let mut callback_slices = Vec::new();
        let mut callbacks_resumed = 0;
        let mut actions = key_actions.iter().peekable();
        let mut keypad = Keypad::default();
        let mut key_events = Vec::new();
        if started {
            'advancing: for step in 0..advance_steps {
                while actions.peek().is_some_and(|action| action.step == step) {
                    let action = actions.next().expect("peeked action");
                    keypad.set(
                        action.key,
                        action.down,
                        u64::from(machine.clock_ms()) * 1000,
                    )?;
                }
                while let Some(event) = keypad.pop() {
                    let initial = machine.deliver_handset_event(event, budget)?;
                    let outcomes = complete_callback(
                        &mut machine,
                        vec![initial],
                        step,
                        budget,
                        callback_resume_limit,
                        &mut callbacks_resumed,
                        &mut callback_slices,
                    )?;
                    key_events.push(json!({"step":step, "event":event,
                        "outcomes":outcomes.iter().map(outcome).collect::<Vec<_>>()}));
                    if outcomes
                        .iter()
                        .any(|value| !matches!(value, Outcome::Returned { .. }))
                    {
                        steps.push(outcomes.iter().map(outcome).collect::<Vec<_>>());
                        break 'advancing;
                    }
                }
                let initial = machine.advance(budget)?;
                let outcomes = complete_callback(
                    &mut machine,
                    initial,
                    step,
                    budget,
                    callback_resume_limit,
                    &mut callbacks_resumed,
                    &mut callback_slices,
                )?;
                let stopped = outcomes
                    .iter()
                    .any(|value| !matches!(value, Outcome::Returned { .. }));
                steps.push(outcomes.iter().map(outcome).collect::<Vec<_>>());
                if stopped {
                    break;
                }
            }
        }
        report["callback_steps"] = json!(steps);
        report["callback_slices"] = json!(callback_slices);
        report["callback_resumptions"] = json!(callbacks_resumed);
        report["key_events"] = json!(key_events);
        report["bounded_resumptions"] = json!(resumptions);
        report["api_calls"] = json!(machine.api_calls());
        report["instructions"] = json!(machine.cpu().instructions());
        report["api_trace"] = json!(machine.trace());
        report["media_log"] = json!(machine.media_log());
        if args.iter().any(|arg| arg == "--profile-memory") {
            report["memory_callbacks"] = machine.cpu().relato_de_memoria().map(|counts| json!({
                "data_reads": counts.leituras, "data_writes": counts.escritas,
                "watched_reads": counts.leituras_vigiadas, "watched_writes": counts.escritas_vigiadas,
                "executed_page_reads": counts.leituras_em_codigo,
                "executed_page_writes": counts.escritas_em_codigo,
                "includes_direct_accesses": false,
                "categories_may_overlap": true,
            })).unwrap_or(serde_json::Value::Null);
        }
        report["registers"] = json!(
            [
                Reg::R0,
                Reg::R1,
                Reg::R2,
                Reg::R3,
                Reg::R4,
                Reg::R5,
                Reg::R6,
                Reg::R7,
                Reg::R8,
                Reg::R9,
                Reg::R10,
                Reg::R11,
                Reg::R12,
                Reg::Sp,
                Reg::Lr,
                Reg::Pc
            ]
            .map(|reg| machine.cpu().read_reg(reg))
        );
        report["cpsr"] = json!(machine.cpu().cpsr());
        report["requested_classes"] = json!(machine.requested_classes());
        report["unknown_classes"] = json!(machine.unknown_classes());
        report["fs_operations"] = json!(machine.fs_log().collect::<Vec<_>>());
        report["debug_log"] = json!(machine.debug_output());
        report["inherited_assumptions"] = json!(machine.assumptions());
        let missing_apis = machine.missing_apis();
        let memory_faults = machine.swallowed_faults();
        let bad_pointers = machine.bad_pointers();
        report["diagnostics"] = json!({
            "clean":missing_apis.is_empty() && memory_faults.is_empty() && bad_pointers.is_empty()
                && machine.unknown_classes().is_empty() && machine.assumptions().is_empty(),
            "missing_apis":missing_apis, "memory_faults":memory_faults, "bad_pointers":bad_pointers,
        });
        report["candidate_display"] = json!({"width":machine.screen().width(), "height":machine.screen().height(), "handset_geometry_verified":false});
        report["guest_clock_ms"] = json!(machine.clock_ms());
        if capture {
            let screen = machine.screen();
            let pixels = screen.to_rgb565_bytes();
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(captures.join("final.rgb565"))?;
            file.write_all(&pixels)?;
            report["framebuffer_capture"] = json!({
                "format":"RGB565_LE", "relative_file":"captures/final.rgb565",
                "width":screen.width(), "height":screen.height(),
                "row_stride_bytes":screen.width() * 2, "byte_length":pixels.len(),
                "pixel_writes":screen.escritas(), "signature":format!("{:016x}", screen.signature()),
            });
        }
    }
    let mut destination = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    destination.write_all(&serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("BREW startup probe failed: {error}");
        std::process::exit(1);
    }
}
