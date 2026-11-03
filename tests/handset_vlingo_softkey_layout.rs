//! O consumidor interrompe a montagem antes de Insert quando PROP_FLAGS é recusado.
//! AEECSoftkeyWidget encaminha eventos não tratados aos rótulos; 0x420 é
//! IDF_ALIGN_CENTER | IDF_ALIGN_BOTTOM, documentado em AEEIDisplay.h.
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};

const OUT: u32 = loader::HEAP_BASE + 0x10000;
const FLAGS: u32 = 0x153;
struct Fixture {
    m: Machine<BackendPadrao>,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let base = std::env::var_os("BREW_VLINGO_TEST_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("zeebx-vlingo-softkey"));
        let root = base.join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let storage = StoragePaths::from_root(root.join("runtime"));
        storage.create_dirs().unwrap();
        let image = ModImage::parse(
            [0xe3a00000u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
        let mut m = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            &root,
            &storage,
            None,
        );
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        let mut f = Self { m };
        let slot = (0..256)
            .find(|&s| Interface::Helpers.method(s) == Some("malloc"))
            .unwrap();
        let reserved = f.run(aee::encode(Interface::Helpers, slot), [0x20000, 0, 0, 0]);
        assert!(reserved <= OUT && reserved + 0x20000 > OUT + 4096);
        f
    }
    fn run(&mut self, entry: u32, args: [u32; 4]) -> u32 {
        match self.m.call_guest(entry, args, 500_000).unwrap() {
            Outcome::Returned { code } => code,
            other => panic!("{entry:#x}: {other:?}"),
        }
    }
    fn read(&self, p: u32) -> u32 {
        self.m.cpu().read_u32(p).unwrap()
    }
    fn put(&mut self, p: u32, value: u32) {
        self.m.cpu_mut().write_u32(p, value).unwrap();
    }
    fn call(&mut self, object: u32, slot: u32, args: [u32; 3]) -> u32 {
        let entry = self.read(self.read(object) + slot * 4);
        self.run(entry, [object, args[0], args[1], args[2]])
    }
    fn create(&mut self, class: u32) -> u32 {
        assert_eq!(
            self.run(
                aee::encode(Interface::Shell, 2),
                [loader::OBJECT_BASE, class, OUT, 0]
            ),
            0
        );
        let object = self.read(OUT);
        assert_ne!(object, 0);
        object
    }
    fn get(&mut self, widget: u32, property: u32) -> u32 {
        assert_eq!(self.call(widget, 3, [0x800, property, OUT]), 1);
        self.read(OUT)
    }
    fn set(&mut self, widget: u32, property: u32, value: u32) -> u32 {
        self.call(widget, 3, [0x801, property, value])
    }
    fn extent(&mut self, widget: u32, width: u32, height: u32) {
        self.put(OUT, width);
        self.put(OUT + 4, height);
        self.call(widget, 7, [OUT, 0, 0]);
    }
    fn size(&mut self, widget: u32) -> (u32, u32) {
        self.call(widget, 6, [OUT, 0, 0]);
        (self.read(OUT), self.read(OUT + 4))
    }
    fn text(&mut self, widget: u32, value: &str) {
        assert_eq!(self.call(widget, 12, [0x0101593b, OUT, 0]), 0);
        let model = self.read(OUT);
        let text: Vec<_> = value.encode_utf16().chain([0]).collect();
        self.m
            .cpu_mut()
            .write_mem(
                OUT + 128,
                &text
                    .iter()
                    .flat_map(|u| u.to_le_bytes())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        self.call(model, 5, [OUT + 128, text.len() as u32 - 1, 0]);
        self.call(model, 1, [0; 3]);
    }
    fn attach_like_consumer(&mut self, parent: u32, widget: u32, flags: u32) -> bool {
        // 23fe6..23ff6 condiciona GetPreferredExtent/SetExtent e Insert ao booleano.
        if self.set(widget, FLAGS, flags) == 0 {
            return false;
        }
        self.call(widget, 5, [OUT, 0, 0]);
        let height = self.read(OUT + 4);
        self.extent(widget, 240, height * 2);
        let (_, height) = self.size(widget);
        self.put(OUT, 0);
        self.put(OUT + 4, 320 - height);
        self.put(OUT + 8, 1);
        assert_eq!(self.call(parent, 5, [widget, 0, OUT]), 0);
        true
    }
}

#[test]
fn valid_label_flags_allow_the_consumers_real_extent_and_container_attachment() {
    let mut f = Fixture::new();
    let parent = f.create(0x010551ed);
    let widget = f.create(0x010551df);
    let left = f.get(widget, 0x200);
    let right = f.get(widget, 0x201);
    f.text(left, "Yes");
    f.text(right, "No");
    assert!(
        f.attach_like_consumer(parent, widget, 0x420),
        "PROP_FLAGS=IDF_ALIGN_CENTER|IDF_ALIGN_BOTTOM must not abort before Insert"
    );
    assert_eq!(
        f.get(widget, FLAGS),
        0x420,
        "AddFlags must preserve label flags"
    );
    assert_eq!(f.get(left, FLAGS) & 0x770, 0x420);
    assert_eq!(f.get(right, FLAGS) & 0x770, 0x420);
    let (width, height) = f.size(widget);
    assert_eq!(width, 240);
    assert!(height > 0);
    assert_eq!(f.size(left), (116, height));
    assert_eq!(f.size(right), (116, height));
    f.call(widget, 8, [OUT, 0, 0]);
    assert_eq!(f.read(OUT), parent);
    f.call(parent, 1, [0; 3]); // referência adquirida por GetParent
    assert_eq!(f.call(parent, 7, [0, 1, 0]), widget);
    assert_eq!(f.call(parent, 9, [widget, OUT, 0]), 0);
    assert_eq!(
        (f.read(OUT), f.read(OUT + 4), f.read(OUT + 8) & 255),
        (0, 320 - height, 1)
    );
    let flags = f.get(widget, FLAGS);
    assert_eq!(f.set(widget, FLAGS, flags | 1), 1);
    assert_eq!(f.get(widget, FLAGS), 0x421);
    let center = f.get(widget, 0x254);
    assert_ne!(center, 0);
    assert_eq!(f.get(center, FLAGS) & 0x770, 0x420);
    assert_eq!(f.size(center), (77, height));
    let flags = f.get(widget, FLAGS);
    assert_eq!(f.set(widget, FLAGS, flags & !1), 1);
    assert_eq!(f.get(widget, FLAGS), 0x420);
    assert_eq!(f.size(left), (116, height));
}

#[test]
fn label_alignment_changes_actual_glyph_positions_in_both_softkey_cells() {
    let mut f = Fixture::new();
    let widget = f.create(0x010551df);
    let left = f.get(widget, 0x200);
    let right = f.get(widget, 0x201);
    let font = f.create(0x0103fb4b);
    assert_eq!(f.set(widget, 0x152, font), 1);
    f.text(left, "I");
    f.text(right, "I");
    assert_eq!(f.set(left, 0x140, 0x0000ff00), 1);
    assert_eq!(f.set(right, 0x140, 0x00ff0000), 1);
    assert_eq!(f.set(widget, FLAGS, 0x420), 1);
    f.extent(widget, 80, 40);
    let display = f.create(0x01001001);
    f.call(display, 16, [OUT, 0, 0]);
    let bitmap = f.read(OUT);
    assert_eq!(f.call(bitmap, 2, [0x0101e443, OUT, 0]), 0);
    let canvas = f.read(OUT);
    f.call(widget, 10, [canvas, 3, 4]);
    for (color, xrange) in [(0xf800, 17..33), (0x07e0, 53..69)] {
        let mut count = 0;
        for y in 0..48 {
            for x in 0..88 {
                if f.m.screen().get_pixel(x, y) == color {
                    count += 1;
                    assert!(
                        xrange.contains(&x),
                        "glyph must be centered in its cell: {x},{y}"
                    );
                    assert!(
                        (28..44).contains(&y),
                        "glyph must be bottom aligned: {x},{y}"
                    );
                }
            }
        }
        assert!(count > 0, "the real label must draw its glyph");
    }
}

#[test]
fn unsupported_flag_bits_remain_false_and_do_not_mutate_or_attach() {
    let mut f = Fixture::new();
    let parent = f.create(0x010551ed);
    let widget = f.create(0x010551df);
    let left = f.get(widget, 0x200);
    let right = f.get(widget, 0x201);
    let before = (f.get(left, FLAGS), f.get(right, FLAGS), f.size(widget));
    assert!(!f.attach_like_consumer(parent, widget, 0x80000000));
    assert!(!f.attach_like_consumer(parent, widget, 0x80000421));
    assert_eq!(f.get(widget, FLAGS), 0);
    assert_eq!(
        f.get(widget, 0x254),
        0,
        "rejeição não cria o rótulo central"
    );
    assert_eq!(
        (f.get(left, FLAGS), f.get(right, FLAGS), f.size(widget)),
        before
    );
    f.call(widget, 8, [OUT, 0, 0]);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(parent, 7, [0, 1, 0]), 0);
    assert_eq!(f.set(widget, 0xffff, 0), 0);
}

#[test]
fn center_flag_round_trip_preserves_the_existing_independent_label_layout() {
    let mut f = Fixture::new();
    let widget = f.create(0x010551df);
    let left = f.get(widget, 0x200);
    let right = f.get(widget, 0x201);
    assert_eq!(f.set(widget, FLAGS, 1), 1);
    let center = f.get(widget, 0x254);
    assert_ne!(center, 0);
    assert_eq!(f.get(widget, FLAGS), 1);
    assert_eq!(f.get(left, FLAGS) & 0x770, 0x210);
    assert_eq!(f.get(center, FLAGS) & 0x770, 0x220);
    assert_eq!(f.get(right, FLAGS) & 0x770, 0x240);
    f.extent(widget, 80, 24);
    assert_eq!(f.size(left), (24, 24));
    assert_eq!(f.size(center), (24, 24));
    assert_eq!(f.size(right), (24, 24));
    assert_eq!(f.set(widget, FLAGS, 0), 1);
    assert_eq!(f.get(widget, FLAGS), 0);
    assert_eq!(f.size(left), (36, 24));
    assert_eq!(f.size(right), (36, 24));
}
