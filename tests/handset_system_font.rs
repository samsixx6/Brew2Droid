use std::path::PathBuf;
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

#[test]
fn silent_midi_com_volume_zero_e_um_recurso_valido() {
    // DMC usa uma nota com CC7=0 para inicializar o tocador. Silêncio não é erro de codec.
    let track = [0,0xb0,7,0, 0,0x90,60,100, 96,0x80,60,64, 0,0xff,0x2f,0];
    let mut midi = b"MThd\0\0\0\x06\0\0\0\x01\0\x60MTrk".to_vec();
    midi.extend_from_slice(&(track.len() as u32).to_be_bytes());
    midi.extend_from_slice(&track);
    let sound = zeebx::audio::midi::decode(&midi).expect("MIDI silencioso ainda tem duração e término");
    assert!(sound.samples.len() >= sound.rate as usize / 2);
    assert!(sound.samples.iter().all(|value| *value == 0.0));
}

#[test]
fn handset_sem_fonte_do_console_rasteriza_texto_do_menu() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests")
        .join(format!("handset-font-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let storage = StoragePaths::from_root(root.join("runtime"));
    let image = ModImage::parse([0xe3a0_0000u32, 0xe12f_ff1e]
        .into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
    let mut machine = Machine::new_with_storage(BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(), &root, &storage, None);
    machine.enable_handset_clock();
    machine.run(1000).unwrap();
    let text = loader::HEAP_BASE + 0x100;
    machine.cpu_mut().write_mem(text, &[b'M',0,b'e',0,b'n',0,b'u',0,0,0]).unwrap();
    let sp = machine.cpu().read_reg(Reg::Sp) - 32;
    machine.cpu_mut().write_mem(sp, &[0;32]).unwrap();
    machine.cpu_mut().write_reg(Reg::Sp, sp);
    let display = loader::OBJECT_BASE + 0x10;
    for (name,args) in [("SetColor",[display,1,0xffff_ff00,0]),
        ("DrawText",[display,0x8000,text,4]), ("Update",[display,0,0,0])] {
        let slot = (0..256).find(|slot| Interface::Display.method(*slot) == Some(name)).unwrap();
        assert!(matches!(machine.call_guest(aee::encode(Interface::Display,slot),args,1000).unwrap(),
            Outcome::Returned {..}));
    }
    assert!((0..40).any(|y| (0..80).any(|x| machine.screen().get_pixel(x,y) != 0)),
        "DrawText precisa produzir pixels mesmo sem ROM ou fonte do console");
    drop(machine);
    assert!(root.canonicalize().unwrap().starts_with(PathBuf::from(env!("CARGO_MANIFEST_DIR")).canonicalize().unwrap()));
    std::fs::remove_dir_all(root).unwrap();
}
