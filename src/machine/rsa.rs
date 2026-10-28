//! IRSA do SDK: chaves copiadas, resultado interno e conclusão pelo laço BREW.
use super::*;
use crate::brew::rsa as math;
use crate::save_state::{Erro, Leitor, Secoes};

const CANCEL_STUB: u32 = loader::STUB_BASE + 0xa0;
const INVALID_KEY: u32 = 0x603;
const INVALID_SIZE: u32 = 0x609;
const INVALID_PADDING: u32 = 0x604;
#[derive(Debug, Clone)]
pub(super) struct Pending {
    kind: u32,
    args: [u32; 7],
}
#[derive(Debug, Default)]
pub(super) struct RsaState {
    modulus: Vec<u8>,
    exponent: Vec<u8>,
    output: u32,
    capacity: u32,
    pending: Option<Pending>,
}
fn error_code(error: math::Error) -> u32 {
    match error {
        math::Error::Key => INVALID_KEY,
        math::Error::Size => INVALID_SIZE,
        math::Error::Padding => 0x605,
        math::Error::Signature => 0x401,
        math::Error::Unsupported => EUNSUPPORTED,
    }
}

impl<C: CpuBackend> Machine<C> {
    pub(super) fn rsa_pending(&self) -> bool {
        self.rsas.values().any(|state| state.pending.is_some())
    }

    fn rsa_clear_callback(&mut self, owner: u32, cb: u32) -> Result<(), CpuError> {
        if cb != 0
            && cb.checked_add(CALLBACK_SIZE).is_some()
            && self.cpu.read_u32(cb + 8).ok() == Some(CANCEL_STUB)
            && self.cpu.read_u32(cb + 12).ok() == Some(owner)
        {
            self.cpu.write_u32(cb + 8, 0)?;
            self.cpu.write_u32(cb + 12, 0)?;
        }
        Ok(())
    }
    fn rsa_cancel(&mut self, owner: u32) -> Result<(), CpuError> {
        if let Some(pending) = self
            .rsas
            .get_mut(&owner)
            .and_then(|state| state.pending.take())
        {
            self.rsa_clear_callback(owner, pending.args[6])?;
        }
        Ok(())
    }
    fn rsa_schedule(&mut self, owner: u32, kind: u32, args: [u32; 7]) -> Result<(), CpuError> {
        self.rsa_cancel(owner)?;
        let cb = args[6];
        if cb != 0 {
            cb.checked_add(CALLBACK_SIZE)
                .ok_or_else(|| CpuError("callback RSA fora da memória".into()))?;
            let mut bytes = [0; CALLBACK_SIZE as usize];
            self.cpu.read_mem(cb, &mut bytes)?;
            // CALLBACK_Cancel chama pfnCancel(pcb). A escrita da marca é suficiente:
            // a operação continua pendente e cancelável até poll_rsa, fora da API.
            let code = [
                0xe3a0_1000u32,
                0xe580_1008,
                0xe580_100c,
                0xe3a0_0000,
                0xe12f_ff1e,
            ];
            self.cpu.write_mem(
                CANCEL_STUB,
                &code
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )?;
            self.cpu.write_u32(cb + 8, CANCEL_STUB)?;
            self.cpu.write_u32(cb + 12, owner)?;
        }
        self.rsas.get_mut(&owner).unwrap().pending = Some(Pending { kind, args });
        Ok(())
    }
    fn rsa_raw(
        &mut self,
        owner: u32,
        pointer: u32,
        length: u32,
    ) -> Result<Result<Vec<u8>, u32>, CpuError> {
        let state = &self.rsas[&owner];
        if !math::valid_key(&state.modulus, &state.exponent) {
            return Ok(Err(INVALID_KEY));
        }
        if length as usize != state.modulus.len() {
            return Ok(Err(INVALID_SIZE));
        }
        let n = state.modulus.clone();
        let e = state.exponent.clone();
        let input = self.read_bytes(pointer, length)?;
        Ok(math::mod_exp(&n, &e, &input).map_err(error_code))
    }
    fn rsa_output(&mut self, owner: u32, bytes: &[u8]) -> Result<Result<u32, u32>, CpuError> {
        let old = self.rsas[&owner].output;
        let mut pointer = old;
        if pointer == 0 || self.rsas[&owner].capacity < bytes.len() as u32 {
            pointer = self.malloc(bytes.len() as u32)?;
            if pointer == 0 {
                return Ok(Err(ENOMEMORY));
            }
            if old != 0 {
                self.heap.free(old);
            }
            let state = self.rsas.get_mut(&owner).unwrap();
            state.output = pointer;
            state.capacity = bytes.len() as u32;
        }
        self.cpu.write_mem(pointer, bytes)?;
        Ok(Ok(pointer))
    }
    pub(super) fn rsa_call(&mut self, slot: u32) -> Result<u32, CpuError> {
        let owner = self.arg(0);
        if self.objects.kind_of(owner) != Some(Interface::Rsa) {
            return Ok(EBADPARM);
        }
        match slot {
            0 => Ok(self.objects.add_ref(owner)),
            1 => {
                let refs = self.objects.release(owner);
                if refs == 0 {
                    self.rsa_cancel(owner)?;
                    if let Some(state) = self.rsas.remove(&owner) {
                        if state.output != 0 {
                            self.heap.free(state.output);
                        }
                    }
                }
                Ok(refs)
            }
            2 => {
                let (n, len, e, elen) = (self.arg(1), self.arg(2), self.arg(3), self.arg(4));
                if n == 0
                    || e == 0
                    || len == 0
                    || elen == 0
                    || len as usize > math::MAX_BYTES
                    || elen as usize > math::MAX_BYTES
                {
                    return Ok(INVALID_KEY);
                }
                let modulus = self.read_bytes(n, len)?;
                let exponent = self.read_bytes(e, elen)?;
                if !math::valid_key(&modulus, &exponent) {
                    return Ok(INVALID_KEY);
                }
                self.rsa_cancel(owner)?;
                let state = self.rsas.get_mut(&owner).unwrap();
                if state.output != 0 {
                    self.heap.free(state.output);
                }
                *state = RsaState {
                    modulus,
                    exponent,
                    ..Default::default()
                };
                Ok(SUCCESS)
            }
            3 => {
                let (src, len, dest, size, reserved) = (
                    self.arg(1),
                    self.arg(2),
                    self.arg(3),
                    self.arg(4),
                    self.arg(5),
                );
                if reserved != 0 || dest == 0 || size == 0 {
                    return Ok(EBADPARM);
                }
                self.rsa_cancel(owner)?;
                match self.rsa_raw(owner, src, len)? {
                    Err(code) => Ok(code),
                    Ok(bytes) => match self.rsa_output(owner, &bytes)? {
                        Err(code) => Ok(code),
                        Ok(pointer) => {
                            self.cpu.write_u32(dest, pointer)?;
                            self.cpu.write_u32(size, bytes.len() as u32)?;
                            Ok(SUCCESS)
                        }
                    },
                }
            }
            4 => {
                let args = [
                    self.arg(1),
                    self.arg(2),
                    self.arg(3),
                    self.arg(4),
                    0,
                    self.arg(5),
                    self.arg(6),
                ];
                self.rsa_schedule(owner, slot, args)?;
                Ok(SUCCESS)
            }
            5..=7 => {
                let args = [
                    self.arg(1),
                    self.arg(2),
                    self.arg(3),
                    self.arg(4),
                    self.arg(5),
                    self.arg(6),
                    self.arg(7),
                ];
                self.rsa_schedule(owner, slot, args)?;
                Ok(SUCCESS)
            }
            _ => Ok(EUNSUPPORTED),
        }
    }
    pub(super) fn poll_rsa(&mut self, budget: u64) -> Result<Vec<Outcome>, CpuError> {
        // Uma conclusão por volta deixa as demais canceláveis inclusive pelo callback
        // que acaba de rodar. Não há notifier resolvido esperando em uma fila sem dono.
        // Round-robin impede que um callback que reenfileira o próprio objeto
        // adie indefinidamente o trabalho de outro objeto já pendente.
        let pending_ids = || {
            self.rsas
                .iter()
                .filter(|(_, s)| s.pending.is_some())
                .map(|(id, _)| *id)
        };
        let Some(owner) = pending_ids()
            .filter(|id| *id > self.rsa_last_polled)
            .min()
            .or_else(|| pending_ids().min())
        else {
            return Ok(Vec::new());
        };
        self.rsa_last_polled = owner;
        let pending = self.rsas.get_mut(&owner).unwrap().pending.take().unwrap();
        let a = pending.args;
        let cb = a[6];
        if cb != 0
            && (cb.checked_add(CALLBACK_SIZE).is_none()
                || self.cpu.read_u32(cb + 8).ok() != Some(CANCEL_STUB)
                || self.cpu.read_u32(cb + 12).ok() != Some(owner)
                || self.cpu.read_u32(cb + 16).unwrap_or(0) == 0)
        {
            self.rsa_clear_callback(owner, cb)?;
            return Ok(Vec::new());
        }
        let code = match pending.kind {
            4 => {
                if a[2] == 0 || a[3] == 0 {
                    EBADPARM
                } else {
                    match self.rsa_raw(owner, a[0], a[1])? {
                        Err(code) => code,
                        Ok(bytes) => match self.rsa_output(owner, &bytes)? {
                            Err(code) => code,
                            Ok(pointer) => {
                                self.cpu.write_u32(a[2], pointer)?;
                                self.cpu.write_u32(a[3], bytes.len() as u32)?;
                                SUCCESS
                            }
                        },
                    }
                }
            }
            7 => {
                let state = &self.rsas[&owner];
                if !math::valid_key(&state.modulus, &state.exponent) {
                    INVALID_KEY
                } else if !matches!(a[4], 2 | 3) {
                    INVALID_PADDING
                } else if a[1] == 0
                    || a[1] as usize > state.modulus.len()
                    || a[3] != (if a[4] == 2 { 16 } else { 20 })
                {
                    INVALID_SIZE
                } else {
                    let (n, e) = (state.modulus.clone(), state.exponent.clone());
                    let signature = self.read_bytes(a[0], a[1])?;
                    let digest = self.read_bytes(a[2], a[3])?;
                    math::verify_digest(&n, &e, &signature, &digest, a[4])
                        .map_or_else(error_code, |_| SUCCESS)
                }
            }
            // Os modos com outros envelopes ficam explícitos, nunca sucesso fictício.
            _ => EUNSUPPORTED,
        };
        self.rsa_clear_callback(owner, cb)?;
        self.write_at(a[5], code)?;
        if cb == 0 {
            return Ok(Vec::new());
        }
        let call = self.resolve_notify(Callback {
            function: cb,
            context: cb,
        })?;
        if call.function == 0 {
            return Ok(Vec::new());
        }
        Ok(vec![self.call_guest(
            call.function,
            [call.context, 0, 0, 0],
            budget,
        )?])
    }
    pub(super) fn grava_rsa(&self, sections: &mut Secoes) {
        sections.poe_u32s("rsa.ids", self.rsas.keys().copied());
        sections.poe_u32s("rsa.cursor", [self.rsa_last_polled]);
        for (id, state) in &self.rsas {
            sections.poe(&format!("rsa.{id}.n"), state.modulus.clone());
            sections.poe(&format!("rsa.{id}.e"), state.exponent.clone());
            sections.poe_u32s(&format!("rsa.{id}.buffer"), [state.output, state.capacity]);
            let pending = state
                .pending
                .as_ref()
                .map(|p| std::iter::once(p.kind).chain(p.args).collect::<Vec<_>>())
                .unwrap_or_default();
            sections.poe_u32s(&format!("rsa.{id}.pending"), pending);
        }
    }
    pub(super) fn restaura_rsa(&mut self, reader: &Leitor<'_>) -> Result<(), Erro> {
        let mut restored = HashMap::new();
        let mut buffers = HashSet::new();
        if reader.secao("rsa.ids").is_some() {
            for id in reader.u32s("rsa.ids")? {
                let bad = || Erro::Secao {
                    nome: format!("rsa.{id}"),
                    motivo: "estado IRSA inválido".into(),
                };
                if restored.contains_key(&id) || self.objects.kind_of(id) != Some(Interface::Rsa) {
                    return Err(bad());
                }
                let n = reader.secao(&format!("rsa.{id}.n")).ok_or_else(bad)?;
                let e = reader.secao(&format!("rsa.{id}.e")).ok_or_else(bad)?;
                if !(n.is_empty() && e.is_empty()) && !math::valid_key(n, e) {
                    return Err(bad());
                }
                let output = reader.u32s(&format!("rsa.{id}.buffer"))?;
                let pending = reader.u32s(&format!("rsa.{id}.pending"))?;
                if output.len() != 2
                    || (output[0] == 0) != (output[1] == 0)
                    || output[1] as usize > math::MAX_BYTES
                    || (output[0] != 0
                        && (self.heap.size_of(output[0]).unwrap_or(0) < output[1]
                            || !buffers.insert(output[0])))
                    || !(pending.is_empty()
                        || (pending.len() == 8
                            && (4..=7).contains(&pending[0])
                            && (pending[7] == 0
                                || pending[7].checked_add(CALLBACK_SIZE).is_some())))
                {
                    return Err(bad());
                }
                restored.insert(
                    id,
                    RsaState {
                        modulus: n.to_vec(),
                        exponent: e.to_vec(),
                        output: output[0],
                        capacity: output[1],
                        pending: if pending.is_empty() {
                            None
                        } else {
                            Some(Pending {
                                kind: pending[0],
                                args: pending[1..].try_into().unwrap(),
                            })
                        },
                    },
                );
            }
        }
        let cursor = if reader.secao("rsa.cursor").is_some() {
            let values = reader.u32s("rsa.cursor")?;
            if values.len() != 1 {
                return Err(Erro::Secao {
                    nome: "rsa.cursor".into(),
                    motivo: "cursor IRSA inválido".into(),
                });
            }
            values[0]
        } else {
            0
        };
        self.rsas = restored;
        self.rsa_last_polled = cursor;
        Ok(())
    }
}
