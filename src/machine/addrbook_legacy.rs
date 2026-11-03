//! IAddrBook do SDK móvel: a agenda virtual não consulta os contatos do anfitrião.

use super::*;

const SDK_BAD_PARAM: u32 = 14;
const SDK_NO_MEMORY: u32 = 2;
const ADDRBOOK_BAD_STATE: u32 = 0x30e;
const LAST_ERROR: u32 = 4;
const PROPERTIES: u32 = 8;
const ENUM_INITIALIZED: u32 = 12;

impl<C: CpuBackend> Machine<C> {
    pub(super) fn cria_addrbook_legado(&mut self, out: u32) -> Result<u32, CpuError> {
        if out == 0 {
            return Ok(SDK_BAD_PARAM);
        }
        let object = self.new_object(Interface::AddrBookLegacy)?;
        self.cpu.write_u32(out, object)?;
        if object == 0 {
            return Ok(SDK_NO_MEMORY);
        }
        // O slot opaco é reutilizado. Os três campos pertencem à nova interface e
        // entram no save state com a memória ARM, sem contatos ou arquivos do host.
        self.cpu.write_mem(object + LAST_ERROR, &[0; 12])?;
        Ok(SUCCESS)
    }

    pub(super) fn addrbook_legado_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let object = self.cpu.read_reg(Reg::R0);
        if self.objects.kind_of(object) != Some(Interface::AddrBookLegacy) {
            return Ok(Some(SDK_BAD_PARAM));
        }
        // AEEAddrBook.h usa DECLARE_IBASE, portanto o slot 2 já é EnumCategoryInit.
        let result = match slot {
            0 => self.objects.add_ref(object),
            1 => {
                let remaining = self.objects.release(object);
                if remaining == 0 {
                    self.cpu.write_mem(object + LAST_ERROR, &[0; 12])?;
                }
                remaining
            }
            // Orcs & Elves II consulta este total antes de alocar a lista de nomes.
            7 => 0,
            8 => {
                // RemoveAllRecs admite EUNSUPPORTED explicitamente no SDK. A agenda
                // vazia não anuncia capacidade de gravar/apagar contatos.
                self.cpu.write_u32(object + LAST_ERROR, EUNSUPPORTED)?;
                EUNSUPPORTED
            }
            10 => {
                let category = self.cpu.read_reg(Reg::R1) as u16;
                let field = self.cpu.read_reg(Reg::R2) as u16;
                let data = self.cpu.read_reg(Reg::R3);
                let size = self.stack_arg(0)? as u16;
                // O chamador inspecionado usa exatamente a enumeração de todos os registros.
                // Filtros e enumeração estendida precisam de semântica própria: None
                // conserva o diagnóstico, em vez de fingir que a busca foi executada.
                if category != 0 || field != 0 || data != 0 || size != 0 {
                    return Ok(None);
                }
                self.cpu.write_u32(object + LAST_ERROR, SUCCESS)?;
                self.cpu.write_u32(object + ENUM_INITIALIZED, 1)?;
                SUCCESS
            }
            11 => {
                let initialized = self.cpu.read_u32(object + ENUM_INITIALIZED)? != 0;
                self.cpu.write_u32(
                    object + LAST_ERROR,
                    if initialized {
                        SUCCESS
                    } else {
                        ADDRBOOK_BAD_STATE
                    },
                )?;
                // IAddrRec*: fim de uma agenda vazia é NULL, nunca um código de erro.
                0
            }
            12 => self.cpu.read_u32(object + LAST_ERROR)?,
            13 => {
                self.cpu
                    .write_u32(object + PROPERTIES, self.cpu.read_reg(Reg::R1))?;
                SUCCESS
            }
            14 => self.cpu.read_u32(object + PROPERTIES)?,
            // Não criar registros, metadados ou nomes fictícios para métodos não usados.
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::cpu::BackendPadrao;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn machine() -> (Machine<BackendPadrao>, crate::scratch::TempDir) {
        let root = crate::scratch::TempDir::new(&format!(
            "brew-addrbook-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let image = loader::modfile::ModImage::parse(
            [0xe3a0_0000u32, 0xe12f_ff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
        let storage = crate::storage::StoragePaths::from_root(root.join("runtime"));
        let mut m = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            root.path(),
            &storage,
            None,
        );
        m.run(1000).unwrap();
        (m, root)
    }
    fn invoke(m: &mut Machine<BackendPadrao>, function: u32, regs: [u32; 4], extra: &[u32]) -> u32 {
        match m
            .call_guest_with_stack(function, regs, extra, 10000)
            .unwrap()
        {
            Outcome::Returned { code } => code,
            other => panic!("address book: {other:?}"),
        }
    }
    fn create(m: &mut Machine<BackendPadrao>) -> u32 {
        let out = loader::STACK_BASE + 0x100;
        assert_eq!(
            invoke(
                m,
                aee::encode(Interface::Shell, 2),
                [loader::OBJECT_BASE, 0x0100_2017, out, 0],
                &[]
            ),
            0
        );
        let object = m.cpu.read_u32(out).unwrap();
        assert_ne!(object, 0);
        object
    }
    fn call(
        m: &mut Machine<BackendPadrao>,
        object: u32,
        slot: u32,
        args: [u32; 3],
        extra: &[u32],
    ) -> u32 {
        let table = m.cpu.read_u32(object).unwrap();
        let function = m.cpu.read_u32(table + slot * 4).unwrap();
        invoke(m, function, [object, args[0], args[1], args[2]], extra)
    }

    #[test]
    fn addrbook_factory_returns_a_refcounted_empty_virtual_store() {
        let (mut m, _root) = machine();
        let object = create(&mut m);
        // AEEAddrBook.h herda IBase de dois slots, sem QueryInterface intermediário.
        assert_eq!(call(&mut m, object, 7, [0, 0, 0], &[]), 0);
        assert_eq!(call(&mut m, object, 0, [0, 0, 0], &[]), 2);
        assert_eq!(call(&mut m, object, 1, [0, 0, 0], &[]), 1);
        assert_eq!(call(&mut m, object, 1, [0, 0, 0], &[]), 0);
        let fresh = create(&mut m);
        assert_eq!(call(&mut m, fresh, 7, [0, 0, 0], &[]), 0);
    }

    #[test]
    fn addrbook_empty_enumeration_and_unsupported_bulk_delete_are_distinct() {
        let (mut m, _root) = machine();
        let object = create(&mut m);
        assert_eq!(call(&mut m, object, 11, [0, 0, 0], &[]), 0);
        assert_eq!(call(&mut m, object, 12, [0, 0, 0], &[]), 0x30e);
        assert_eq!(call(&mut m, object, 10, [0, 0, 0], &[0]), 0);
        assert_eq!(
            call(&mut m, object, 11, [0, 0, 0], &[]),
            0,
            "fim da agenda vazia é NULL, não ponteiro de erro"
        );
        assert_eq!(call(&mut m, object, 12, [0, 0, 0], &[]), 0);
        // RemoveAllRecs tem EUNSUPPORTED documentado; não anunciar exclusão de dados.
        assert_eq!(call(&mut m, object, 8, [0, 0, 0], &[]), 20);
        assert_eq!(call(&mut m, object, 12, [0, 0, 0], &[]), 20);
        assert_eq!(call(&mut m, object, 10, [0, 0, 0], &[0]), 0);
        assert_eq!(call(&mut m, object, 12, [0, 0, 0], &[]), 0);
    }

    #[test]
    fn addrbook_properties_and_last_error_survive_save_without_host_contacts() {
        let (mut m, _root) = machine();
        let object = create(&mut m);
        call(&mut m, object, 13, [1, 0, 0], &[]);
        assert_eq!(call(&mut m, object, 14, [0, 0, 0], &[]), 1);
        assert_eq!(call(&mut m, object, 10, [0, 0, 0], &[0]), 0);
        assert_eq!(call(&mut m, object, 8, [0, 0, 0], &[]), 20);
        let saved = m.grava_estado();
        call(&mut m, object, 13, [0, 0, 0], &[]);
        m.restaura_estado(&saved).unwrap();
        assert_eq!(call(&mut m, object, 14, [0, 0, 0], &[]), 1);
        assert_eq!(call(&mut m, object, 12, [0, 0, 0], &[]), 20);
        assert_eq!(call(&mut m, object, 11, [0, 0, 0], &[]), 0);
        assert_eq!(call(&mut m, object, 12, [0, 0, 0], &[]), 0);
        let fresh = create(&mut m);
        assert_eq!(call(&mut m, fresh, 14, [0, 0, 0], &[]), 0);
        assert_eq!(call(&mut m, fresh, 11, [0, 0, 0], &[]), 0);
        assert_eq!(call(&mut m, fresh, 12, [0, 0, 0], &[]), 0x30e);
    }
}
