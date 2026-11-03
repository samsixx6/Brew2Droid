//! IDateCtl e ITimeCtl preservam a ABI anterior a QueryInterface.
//! O calendário explícito não compartilha a época de segundos dos helpers.

use super::*;

const SECTION: &str = "tab.temporal_controls";
const FIELDS: usize = 9;
const MAX_CONTROLS: usize = 256;
const SDK_BAD_PARAM: u32 = 14;
const SDK_NO_MEMORY: u32 = 2;

#[derive(Clone)]
pub(super) struct TemporalControl {
    // A documentação não fixa a data inicial. Só publicar valores definidos
    // pelo cliente evita inventar "hoje" ou a época como conteúdo do controle.
    date: Option<i32>,
    rect: Option<[i16; 4]>,
}

fn to_julian(year: i32, month: u32, day: u32) -> Option<i32> {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let doy = (153 * month + 2) / 5 + i64::from(day) - 1;
    // AEEDate.h: primeiro domingo anterior a 1970-01-01 = 1969-12-28.
    // Ao aceitar dia1..31, a soma normaliza 31/abril para1/maio, como o SDK.
    i32::try_from(era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468 + 4).ok()
}

fn from_julian(day: i32) -> [i32; 3] {
    let z = i64::from(day) - 4 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    [
        (year + i64::from(month <= 2)) as i32,
        month as i32,
        day as i32,
    ]
}

impl<C: CpuBackend> Machine<C> {
    pub(super) fn create_temporal_control(
        &mut self,
        interface: Interface,
        out: u32,
    ) -> Result<u32, CpuError> {
        if out == 0 {
            return Ok(SDK_BAD_PARAM);
        }
        self.cpu.write_u32(out, 0)?;
        if self.temporal_controls.len() >= MAX_CONTROLS {
            return Ok(SDK_NO_MEMORY);
        }
        let object = self.new_object(interface)?;
        if object == 0 {
            return Ok(SDK_NO_MEMORY);
        }
        // Ambos os controles documentam a tela inteira como retângulo inicial.
        let rect = i16::try_from(self.screen().width())
            .ok()
            .zip(i16::try_from(self.screen().height()).ok())
            .map(|(width, height)| [0, 0, width, height]);
        self.temporal_controls
            .insert(object, TemporalControl { date: None, rect });
        self.cpu.write_u32(out, object)?;
        Ok(SUCCESS)
    }

    pub(super) fn temporal_control_call(
        &mut self,
        interface: Interface,
        slot: u32,
    ) -> Result<Option<u32>, CpuError> {
        let Some(name) = interface.method(slot) else {
            return Ok(None);
        };
        let object = self.cpu.read_reg(Reg::R0);
        if self.objects.kind_of(object) != Some(interface)
            || !self.temporal_controls.contains_key(&object)
        {
            return Ok(match name {
                "AddRef" | "Release" => Some(0),
                _ => None,
            });
        }
        let a1 = self.cpu.read_reg(Reg::R1);
        let a2 = self.cpu.read_reg(Reg::R2);
        let a3 = self.cpu.read_reg(Reg::R3);
        let result = match name {
            "AddRef" => self.objects.add_ref(object),
            "Release" => {
                let count = self.objects.release(object);
                if count == 0 {
                    self.temporal_controls.remove(&object);
                }
                count
            }
            "SetRect" => {
                let Some(rect) = self.read_rect(a1)? else {
                    return Ok(Some(SDK_BAD_PARAM));
                };
                if rect.width < 0 || rect.height < 0 {
                    return Ok(Some(SDK_BAD_PARAM));
                }
                self.temporal_controls.get_mut(&object).unwrap().rect =
                    Some([rect.x, rect.y, rect.width, rect.height]);
                SUCCESS
            }
            "GetRect" => {
                let Some(rect) = self.temporal_controls[&object].rect else {
                    return Ok(None);
                };
                if a1 == 0 {
                    return Ok(Some(SDK_BAD_PARAM));
                }
                self.cpu.write_mem(
                    a1,
                    &rect
                        .into_iter()
                        .flat_map(i16::to_le_bytes)
                        .collect::<Vec<_>>(),
                )?;
                SUCCESS
            }
            "SetDate" if interface == Interface::DateLegacy => {
                if !(1..=12).contains(&a2) || !(1..=31).contains(&a3) {
                    return Ok(Some(FALSE));
                }
                // Não supor uma política para ano zero/antes da era comum, que
                // o SDK não documenta. Inteiros não representáveis falham sem mutar.
                if a1 as i32 <= 0 {
                    return Ok(None);
                }
                let Some(day) = to_julian(a1 as i32, a2, a3) else {
                    return Ok(Some(FALSE));
                };
                self.temporal_controls.get_mut(&object).unwrap().date = Some(day);
                TRUE
            }
            "SetJulianDay" if interface == Interface::DateLegacy => {
                if from_julian(a1 as i32)[0] <= 0 {
                    return Ok(None);
                }
                self.temporal_controls.get_mut(&object).unwrap().date = Some(a1 as i32);
                TRUE
            }
            "GetDate" if interface == Interface::DateLegacy => {
                let Some(day) = self.temporal_controls[&object].date else {
                    return Ok(None);
                };
                // Validar todos os destinos antes de publicar o primeiro campo.
                if [a1, a2, a3].iter().any(|&p| {
                    p == 0
                        || !self.module.mem.regions().iter().any(|region| {
                            region.writable
                                && u64::from(p) >= u64::from(region.base)
                                && u64::from(p) + 4
                                    <= u64::from(region.base) + region.bytes.len() as u64
                        })
                        || self.cpu.read_u32(p).is_err()
                }) {
                    return Ok(Some(FALSE));
                }
                for (pointer, value) in [a1, a2, a3].into_iter().zip(from_julian(day)) {
                    self.cpu.write_u32(pointer, value as u32)?;
                }
                TRUE
            }
            "GetJulianDay" if interface == Interface::DateLegacy => {
                let Some(day) = self.temporal_controls[&object].date else {
                    return Ok(None);
                };
                day as u32
            }
            "GetDayOfWeek" if interface == Interface::DateLegacy => {
                let Some(day) = self.temporal_controls[&object].date else {
                    return Ok(None);
                };
                day.rem_euclid(7) as u32
            }
            // SetTime desenha e, com bIncUpdate, guarda a string renderizada.
            // Retornar sucesso só por guardar o inteiro esconderia essa obrigação.
            // Redraw, edição e formatos não implementados seguem o diagnóstico normal.
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    pub(super) fn grava_temporal_controls(&self, sections: &mut crate::save_state::Secoes) {
        sections.poe_registros(
            SECTION,
            self.temporal_controls.iter().map(|(&object, state)| {
                let mut row = vec![
                    object,
                    aee::codigo(self.objects.kind_of(object).unwrap()),
                    u32::from(state.date.is_some()),
                    state.date.unwrap_or(0) as u32,
                    u32::from(state.rect.is_some()),
                ];
                row.extend(state.rect.unwrap_or([0; 4]).map(|v| v as i32 as u32));
                row
            }),
        );
    }

    pub(super) fn restaura_temporal_controls(
        &mut self,
        reader: &crate::save_state::Leitor<'_>,
    ) -> Result<(), crate::save_state::Erro> {
        let invalid = |reason: &str| crate::save_state::Erro::Secao {
            nome: SECTION.into(),
            motivo: reason.into(),
        };
        let live = self
            .objects
            .live_by_kind()
            .into_iter()
            .filter_map(|(kind, count)| {
                matches!(kind, Interface::DateLegacy | Interface::ClockLegacy).then_some(count)
            })
            .sum::<usize>();
        let Some(bytes) = reader.secao(SECTION) else {
            if live != 0 {
                return Err(invalid("controle temporal vivo sem estado"));
            }
            self.temporal_controls.clear();
            return Ok(());
        };
        if live > MAX_CONTROLS || bytes.len() != 4 + live * FIELDS * 4 {
            return Err(invalid("tamanho ou contagem de controles inválido"));
        }
        let rows = reader.registros(SECTION, FIELDS)?;
        if rows.len() != live {
            return Err(invalid("contagem de registros inválida"));
        }
        let mut restored = BTreeMap::new();
        for row in rows {
            let interface = aee::de_codigo(row[1]);
            if !matches!(
                interface,
                Some(Interface::DateLegacy | Interface::ClockLegacy)
            ) || self.objects.kind_of(row[0]) != interface
                || self.objects.contagem(row[0]) == 0
                || restored.contains_key(&row[0])
                || row[2] > 1
                || row[4] > 1
                || (row[2] == 0 && row[3] != 0)
                || (row[2] != 0
                    && (interface != Some(Interface::DateLegacy)
                        || from_julian(row[3] as i32)[0] <= 0))
                || (row[4] == 0 && row[5..].iter().any(|&v| v != 0))
                || row[5..].iter().any(|&v| i16::try_from(v as i32).is_err())
                || (row[4] != 0 && ((row[7] as i32) < 0 || (row[8] as i32) < 0))
            {
                return Err(invalid("proprietário ou conteúdo de controle inválido"));
            }
            let date = (row[2] != 0).then_some(row[3] as i32);
            let rect =
                (row[4] != 0).then(|| [row[5] as i16, row[6] as i16, row[7] as i16, row[8] as i16]);
            restored.insert(row[0], TemporalControl { date, rect });
        }
        self.temporal_controls = restored;
        Ok(())
    }
}
