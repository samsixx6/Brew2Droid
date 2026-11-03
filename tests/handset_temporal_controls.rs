use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const OUT: u32 = loader::STACK_BASE + 0x1000;
const DATA: u32 = OUT + 0x100;
const DATE: u32 = 0x01003005;
const CLOCK: u32 = 0x01003007;
const COMMON: &[&str] = &[
    "AddRef",
    "Release",
    "HandleEvent",
    "Redraw",
    "SetActive",
    "IsActive",
    "SetRect",
    "GetRect",
    "SetProperties",
    "GetProperties",
    "Reset",
];
const DATE_METHODS: &[&str] = &[
    "SetTitle",
    "SetJulianDay",
    "GetJulianDay",
    "SetDate",
    "GetDate",
    "GetDayOfWeek",
    "GetDayString",
    "GetMonthString",
    "GetDateString",
    "EnableCommand",
    "SetActiveDayMask",
    "SetFont",
    "GetFont",
    "SizeToFit",
];
const TIME_METHODS: &[&str] = &[
    "SetTime",
    "GetTime",
    "EnableCommand",
    "GetTimeString",
    "SetIncrement",
    "SetEditField",
    "SetFont",
    "GetFont",
    "SizeToFit",
];

struct Fixture {
    machine: Machine<BackendPadrao>,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "brew-date-clock-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let storage = StoragePaths::from_root(root.join("runtime"));
        let image = ModImage::parse(
            [0xe3a00000u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
        let mut machine = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            &root,
            &storage,
            None,
        );
        assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
        Self { machine, root }
    }
    fn invoke(&mut self, function: u32, args: [u32; 4]) -> u32 {
        match self.machine.call_guest(function, args, 10000).unwrap() {
            Outcome::Returned { code } => code,
            other => panic!("{other:?}"),
        }
    }
    fn create(&mut self, class: u32) -> u32 {
        let slot = (0..256)
            .find(|&s| Interface::Shell.method(s) == Some("CreateInstance"))
            .unwrap();
        assert_eq!(
            self.invoke(
                aee::encode(Interface::Shell, slot),
                [loader::OBJECT_BASE, class, OUT, 0]
            ),
            0,
            "classe primária SDK{class:08x}"
        );
        let object = self.read(OUT);
        assert_ne!(object, 0);
        object
    }
    fn function(&self, object: u32, slot: u32) -> u32 {
        self.read(self.read(object) + slot * 4)
    }
    fn call(&mut self, object: u32, slot: u32, args: [u32; 3]) -> u32 {
        self.invoke(
            self.function(object, slot),
            [object, args[0], args[1], args[2]],
        )
    }
    fn read(&self, pointer: u32) -> u32 {
        self.machine.cpu().read_u32(pointer).unwrap()
    }
    fn date(&mut self, object: u32) -> [u32; 3] {
        assert_eq!(self.call(object, 15, [DATA, DATA + 4, DATA + 8]), 1);
        [self.read(DATA), self.read(DATA + 4), self.read(DATA + 8)]
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn primary_date_and_clock_have_exact_sdk_abi_and_real_refcount_lifetimes() {
    let mut f = Fixture::new();
    for (class, methods) in [(DATE, DATE_METHODS), (CLOCK, TIME_METHODS)] {
        let object = f.create(class);
        for (slot, name) in COMMON.iter().chain(methods).enumerate() {
            let (interface, actual_slot) = aee::decode(f.function(object, slot as u32)).unwrap();
            assert_eq!(actual_slot, slot as u32);
            assert_eq!(
                interface.method(actual_slot),
                Some(*name),
                "slot2 é HandleEvent, não QI"
            );
        }
        assert_eq!(f.call(object, 0, [0; 3]), 2);
        assert_eq!(f.call(object, 1, [0; 3]), 1);
        assert_eq!(f.call(object, 1, [0; 3]), 0);
    }
}

#[test]
fn date_set_get_normalizes_calendar_days_and_preserves_state_on_invalid_input() {
    let mut f = Fixture::new();
    let date = f.create(DATE);
    f.machine
        .cpu_mut()
        .write_mem(DATA - 4, &[0xa5; 20])
        .unwrap();
    // AEEDate.h: dias1..31 aceitos; 31/abril normaliza para1/maio.
    assert_eq!(f.call(date, 14, [2024, 4, 31]), 1);
    assert_eq!(f.date(date), [2024, 5, 1]);
    assert_eq!(f.read(DATA - 4), 0xa5a5a5a5);
    assert_eq!(f.read(DATA + 12), 0xa5a5a5a5);
    for input in [[2024, 0, 1], [2024, 13, 1], [2024, 1, 0], [2024, 1, 32]] {
        assert_eq!(f.call(date, 14, input), 0);
        assert_eq!(f.date(date), [2024, 5, 1]);
    }
    assert_eq!(f.call(date, 14, [1900, 2, 29]), 1);
    assert_eq!(f.date(date), [1900, 3, 1]);
    assert_eq!(f.call(date, 14, [2000, 2, 29]), 1);
    assert_eq!(f.date(date), [2000, 2, 29]);
}

#[test]
fn date_julian_days_use_sdk_sunday_before_1970_epoch_and_weekday() {
    let mut f = Fixture::new();
    let date = f.create(DATE);
    assert_eq!(f.call(date, 14, [1970, 1, 1]), 1);
    assert_eq!(
        f.call(date, 13, [0; 3]),
        4,
        "época é28/dez/1969, não época1980 dos helpers"
    );
    assert_eq!(f.call(date, 16, [0; 3]), 4, "quinta-feira=4");
    assert_eq!(f.call(date, 12, [7, 0, 0]), 1);
    assert_eq!(f.date(date), [1970, 1, 4]);
    assert_eq!(f.call(date, 16, [0; 3]), 0);
}

#[test]
fn date_control_instances_and_state_restore_preserve_calendar_and_reference_counts() {
    let mut f = Fixture::new();
    let first = f.create(DATE);
    let second = f.create(DATE);
    assert_eq!(f.call(first, 14, [2000, 2, 29]), 1);
    assert_eq!(f.call(second, 14, [1970, 1, 1]), 1);
    assert_eq!(f.call(first, 0, [0; 3]), 2);
    let saved = f.machine.grava_estado();
    assert_eq!(f.call(first, 14, [2024, 1, 1]), 1);
    assert_eq!(f.call(first, 1, [0; 3]), 1);
    f.machine.restaura_estado(&saved).unwrap();
    assert_eq!(f.date(first), [2000, 2, 29]);
    assert_eq!(f.date(second), [1970, 1, 1]);
    assert_eq!(f.call(first, 1, [0; 3]), 1);
    assert_eq!(f.call(first, 1, [0; 3]), 0);
    assert_eq!(f.call(second, 1, [0; 3]), 0);
}

#[test]
fn unsupported_temporal_rendering_and_clock_settime_stay_diagnostic() {
    for (class, slot, args) in [
        (DATE, 3, [0; 3]),
        (CLOCK, 3, [0; 3]),
        (CLOCK, 11, [3723456, 1, 0]),
    ] {
        let mut f = Fixture::new();
        let object = f.create(class);
        let function = f.function(object, slot);
        assert!(
            matches!(f.machine.call_guest(function,
            [object, args[0], args[1], args[2]], 10000).unwrap(), Outcome::Unimplemented { addr, .. } if addr == function),
            "não aceitar sucesso fictício para desenho/cache não implementado"
        );
    }
}

#[test]
fn unset_dates_unknown_year_semantics_and_clock_values_stay_diagnostic() {
    for (class, slot, args) in [
        (DATE, 13, [0; 3]),
        (DATE, 15, [DATA, DATA + 4, DATA + 8]),
        (DATE, 16, [0; 3]),
        (DATE, 14, [0, 1, 1]),
        (DATE, 14, [u32::MAX, 1, 1]),
        (CLOCK, 12, [0; 3]),
    ] {
        let mut f = Fixture::new();
        let object = f.create(class);
        f.machine.cpu_mut().write_mem(DATA, &[0xa5; 16]).unwrap();
        let outcome = f
            .machine
            .call_guest(
                f.function(object, slot),
                [object, args[0], args[1], args[2]],
                10000,
            )
            .unwrap();
        assert!(matches!(outcome, Outcome::Unimplemented { .. }));
        assert_eq!(f.machine.dump(DATA, 16).unwrap(), vec![0xa5; 16]);
    }
}

#[test]
fn date_get_validates_all_output_words_before_any_write() {
    let mut f = Fixture::new();
    let date = f.create(DATE);
    assert_eq!(f.call(date, 14, [1970, 1, 1]), 1);
    f.machine.cpu_mut().write_mem(DATA, &[0xa5; 12]).unwrap();
    assert_eq!(f.call(date, 15, [DATA, DATA + 4, 0]), 0);
    assert_eq!(f.machine.dump(DATA, 12).unwrap(), vec![0xa5; 12]);
}

#[test]
fn temporal_restore_requires_unique_live_typed_owners_and_canonical_optional_state() {
    let mut f = Fixture::new();
    let date = f.create(DATE);
    let _other_date = f.create(DATE);
    let clock = f.create(CLOCK);
    assert_eq!(f.call(date, 14, [1970, 1, 1]), 1);
    let saved = f.machine.grava_estado();
    let reader = zeebx::save_state::Leitor::abre(&saved).unwrap();
    let section = "tab.temporal_controls";
    let valid = reader.registros(section, 9).unwrap();
    let date_index = valid.iter().position(|row| row[0] == date).unwrap();
    let clock_index = valid.iter().position(|row| row[0] == clock).unwrap();
    let mut duplicate = valid.clone();
    duplicate[1][0] = duplicate[0][0];
    let mut unowned = valid.clone();
    unowned[date_index][0] = loader::OBJECT_BASE;
    let mut bad_presence = valid.clone();
    bad_presence[date_index][2] = 2;
    let mut clock_date = valid.clone();
    clock_date[clock_index][2] = 1;
    clock_date[clock_index][3] = 4;
    let mut bad_rect = valid.clone();
    bad_rect[date_index][7] = 65536;
    for rows in [
        None,
        Some(duplicate),
        Some(unowned),
        Some(bad_presence),
        Some(clock_date),
        Some(bad_rect),
    ] {
        let mut sections = zeebx::save_state::Secoes::nova();
        for name in reader.nomes().into_iter().filter(|name| *name != section) {
            sections.poe(name, reader.secao(name).unwrap().to_vec());
        }
        if let Some(rows) = rows {
            sections.poe_registros(section, rows);
        }
        assert!(f.machine.restaura_estado(&sections.fecha()).is_err());
    }
}
// Fragmento próprio para acrescentar a tests/handset_temporal_controls.rs
// somente depois do build coordenado. Usa Fixture, CLOCK, DATE e DATA já definidos lá.

#[test]
fn review_clock_default_rectangle_is_the_entire_device_screen() {
    let mut f = Fixture::new();
    let clock = f.create(CLOCK);
    let expected = [
        0i16,
        0,
        f.machine.screen().width() as i16,
        f.machine.screen().height() as i16,
    ]
    .into_iter()
    .flat_map(i16::to_le_bytes)
    .collect::<Vec<_>>();
    f.machine
        .cpu_mut()
        .write_mem(DATA - 2, &[0xa5; 12])
        .unwrap();
    let function = f.function(clock, 7); // DECLARE_ICONTROL: GetRect.
    let outcome = f
        .machine
        .call_guest(function, [clock, DATA, 0, 0], 10000)
        .unwrap();
    assert!(
        matches!(outcome, Outcome::Returned { .. }),
        "AEETime.h documenta tela inteira como retângulo inicial; GetRect não pode ficar Unimplemented: {outcome:?}"
    );
    assert_eq!(f.machine.dump(DATA, 8).unwrap(), expected);
    assert_eq!(f.machine.dump(DATA - 2, 2).unwrap(), vec![0xa5; 2]);
    assert_eq!(f.machine.dump(DATA + 8, 2).unwrap(), vec![0xa5; 2]);
}

#[test]
fn review_date_get_rejects_a_readonly_late_output_before_writing_earlier_fields() {
    let mut f = Fixture::new();
    let date = f.create(DATE);
    assert_eq!(f.call(date, 14, [1970, 1, 1]), 1);
    let readonly = loader::VTABLE_BASE;
    let readonly_before = f.machine.cpu().read_u32(readonly).unwrap();
    // O destino é legível, porém não gravável: o loader mapeia as vtables com
    // writable=false. Testar somente read_u32 não valida um ponteiro de saída.
    f.machine
        .cpu_mut()
        .write_mem(DATA - 4, &[0xa5; 20])
        .unwrap();
    let before = f.machine.dump(DATA - 4, 20).unwrap();
    let function = f.function(date, 15); // GetDate(year*, month*, day*).
    let result = f
        .machine
        .call_guest(function, [date, DATA, DATA + 4, readonly], 10000);
    assert_eq!(
        f.machine.dump(DATA - 4, 20).unwrap(),
        before,
        "validar os três destinos antes de publicar ano ou mês"
    );
    assert_eq!(f.machine.cpu().read_u32(readonly).unwrap(), readonly_before);
    assert_eq!(
        result.unwrap(),
        Outcome::Returned { code: 0 },
        "ponteiro de saída inválido devolve FALSE, sem falha parcial de CPU"
    );
}
