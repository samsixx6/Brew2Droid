//! Histórico BREW compartilhado pelos proprietários, com cursor independente por objeto.
use super::*;
use std::sync::{Arc, Mutex};
#[derive(Default)]
pub(crate) struct SharedHistory {
    pub(crate) classes: Vec<u32>,
}
pub(crate) type History = Arc<Mutex<SharedHistory>>;
#[derive(Default)]
pub(super) struct AppHistoryState {
    pub(crate) shared: History,
    pub(crate) managed: bool,
    cursors: HashMap<u32, Option<u32>>,
}
impl AppHistoryState {
    pub(super) fn active(&self) -> Option<u32> {
        self.managed.then(|| {
            self.shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .classes
                .first()
                .copied()
                .unwrap_or(0)
        })
    }
    pub(super) fn created(&mut self, obj: u32) {
        let top = self
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .classes
            .first()
            .copied();
        self.cursors.insert(obj, top);
    }
}
impl<C: CpuBackend> Machine<C> {
    pub(crate) fn set_app_history(&mut self, shared: History) {
        let top = shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .classes
            .first()
            .copied();
        if !self.app_history.managed {
            for cursor in self.app_history.cursors.values_mut() {
                *cursor = top;
            }
        }
        self.app_history.shared = shared;
        self.app_history.managed = true;
    }
    pub(crate) fn takes_return_to_idle(&mut self) -> bool {
        std::mem::take(&mut self.return_to_idle)
    }
    pub(crate) fn history_in_use(&self) -> bool {
        !self.app_history.cursors.is_empty()
    }
    pub(crate) fn app_history_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.cpu.read_reg(Reg::R0);
        let out = self.cpu.read_reg(Reg::R1);
        if slot == 0 {
            return Ok(Some(self.objects.add_ref(this)));
        }
        if slot == 1 {
            let count = self.objects.release(this);
            if count == 0 {
                self.app_history.cursors.remove(&this);
            }
            return Ok(Some(count));
        }
        if slot == 2 {
            let pointer = self.cpu.read_reg(Reg::R2);
            if pointer == 0 {
                return Ok(Some(14));
            }
            if self.cpu.write_u32(pointer, 0).is_err() {
                return Ok(Some(14));
            }
            if out == 0x010396bf {
                if self.cpu.write_u32(pointer, this).is_err() {
                    return Ok(Some(14));
                }
                self.objects.add_ref(this);
                return Ok(Some(0));
            }
            return Ok(Some(20));
        }
        let classes = self
            .app_history
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .classes
            .clone();
        let cursor = self.app_history.cursors.get(&this).copied().flatten();
        let index = cursor.and_then(|class| classes.iter().position(|&c| c == class));
        // AEEIAppHistory.h: Top pode recuperar cursor invalidado pela remoção de outra app.
        if slot == 5 {
            self.app_history
                .cursors
                .insert(this, classes.first().copied());
            return Ok(Some(if classes.is_empty() { 39 } else { 0 }));
        }
        let Some(index) = index else {
            return Ok(Some(if cursor.is_some() { 42 } else { 1 }));
        };
        let result = match slot {
            3 | 4 => {
                let next = if slot == 3 {
                    index.checked_sub(1)
                } else {
                    index.checked_add(1).filter(|&i| i < classes.len())
                };
                if let Some(next) = next {
                    self.app_history.cursors.insert(this, Some(classes[next]));
                    0
                } else {
                    39
                }
            }
            7 | 11 => {
                if out == 0 {
                    14
                } else {
                    if self
                        .cpu
                        .write_u32(
                            out,
                            if slot == 7 {
                                classes[index]
                            } else {
                                index as u32
                            },
                        )
                        .is_err()
                    {
                        14
                    } else {
                        0
                    }
                }
            }
            // Modificações/dados exigem semântica adicional; recusar não altera o histórico.
            6 | 8..=10 | 12..=15 => 20,
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
}
