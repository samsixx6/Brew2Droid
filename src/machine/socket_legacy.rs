//! ISocket de AEENet.h: o invólucro existe antes da conexão de rede.
//! Nenhuma operação usa a rede do anfitrião; as falhas chegam pela ABI do SDK.

use super::*;
use std::collections::VecDeque;

const NET_ERROR: u32 = u32::MAX;
const NET_EBADF: u32 = 0x202;
const NET_EFAULT: u32 = 0x203;
const NET_EOPNOTSUPP: u32 = 0x20a;
const NET_ENOTCONN: u32 = 0x210;
const NET_ENETDOWN: u32 = 0x216;
const NET_EINVAL: u32 = 0x21a;
const SECTION: &str = "tab.socket_legacy.state";
const FIELDS: usize = 13;
const QUEUE_SECTION: &str = "tab.socket_legacy.connect_queue";
const ORDER_SECTION: &str = "tab.socket_legacy.callback_order";
const MAX_CONNECTS_PER_SOCKET: usize = 1024;
const MAX_CONNECTS_TOTAL: usize = 8192;

#[derive(Clone, Copy)]
struct Registration {
    callback: Callback,
    error: u32,
    epoch: u64,
}

#[derive(Clone, Copy)]
enum Channel {
    Connect,
    Readable,
    Writeable,
}

#[derive(Clone)]
pub(super) struct SocketState {
    kind: u32,
    error: u32,
    readable: Option<Registration>,
    writeable: Option<Registration>,
    connect: VecDeque<Registration>,
}

impl SocketState {
    fn registration_mut(&mut self, channel: u32, index: u32) -> Option<&mut Registration> {
        match (channel, index) {
            (0, index) => self.connect.get_mut(index as usize),
            (1, 0) => self.readable.as_mut(),
            (2, 0) => self.writeable.as_mut(),
            _ => None,
        }
    }

    fn take_registration(&mut self, channel: Channel, epoch: u64) -> Option<Registration> {
        match channel {
            Channel::Connect => self
                .connect
                .front()
                .is_some_and(|r| r.epoch == epoch)
                .then(|| self.connect.pop_front())
                .flatten(),
            Channel::Readable | Channel::Writeable => {
                let registration = match channel {
                    Channel::Readable => &mut self.readable,
                    _ => &mut self.writeable,
                };
                if registration.as_ref().is_some_and(|r| r.epoch == epoch) {
                    registration.take()
                } else {
                    None
                }
            }
        }
    }
}

impl<C: CpuBackend> Machine<C> {
    pub(super) fn legacy_socket_callbacks_pending(&self) -> bool {
        self.legacy_sockets.values().any(|state| {
            state.readable.is_some() || state.writeable.is_some() || !state.connect.is_empty()
        })
    }

    pub(super) fn open_legacy_socket(&mut self, kind: u32) -> Result<u32, CpuError> {
        if kind > 1 {
            return Ok(0);
        }
        let object = self.new_object(Interface::SocketLegacy)?;
        if object != 0 {
            self.legacy_sockets.insert(
                object,
                SocketState {
                    kind,
                    error: SUCCESS,
                    readable: None,
                    writeable: None,
                    connect: VecDeque::new(),
                },
            );
        }
        Ok(object)
    }

    fn socket_registration(
        &mut self,
        function: u32,
        context: u32,
        error: u32,
    ) -> Result<Option<Registration>, CpuError> {
        if function == 0 {
            return Ok(None);
        }
        self.legacy_socket_epoch = self
            .legacy_socket_epoch
            .checked_add(1)
            .ok_or_else(|| CpuError("gerações de callback de socket esgotadas".into()))?;
        Ok(Some(Registration {
            callback: Callback { function, context },
            error,
            epoch: self.legacy_socket_epoch,
        }))
    }

    pub(super) fn socket_legacy_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let Some(name) = Interface::SocketLegacy.method(slot) else {
            return Ok(None);
        };
        let object = self.cpu.read_reg(Reg::R0);
        if self.objects.kind_of(object) != Some(Interface::SocketLegacy)
            || !self.legacy_sockets.contains_key(&object)
        {
            // O erro detalhado não é uma contagem positiva de bytes transferidos.
            return Ok(Some(match name {
                "AddRef" | "Release" | "Readable" | "Writeable" | "Cancel" => 0,
                "GetLastError" | "GetPeerName" => NET_EBADF,
                "IOCtl" if self.cpu.read_reg(Reg::R1) == 0x100e => NET_EBADF,
                _ => NET_ERROR,
            }));
        }
        let a1 = self.cpu.read_reg(Reg::R1);
        let a2 = self.cpu.read_reg(Reg::R2);
        let a3 = self.cpu.read_reg(Reg::R3);
        let kind = self.legacy_sockets[&object].kind;
        let result = match name {
            "AddRef" => self.objects.add_ref(object),
            "Release" => {
                let count = self.objects.release(object);
                if count == 0 {
                    self.legacy_sockets.remove(&object);
                }
                count
            }
            "GetLastError" => self.legacy_sockets[&object].error,
            "Readable" | "Writeable" => {
                let registration = self.socket_registration(a1, a2, SUCCESS)?;
                let state = self.legacy_sockets.get_mut(&object).unwrap();
                if name == "Readable" {
                    state.readable = registration;
                } else {
                    state.writeable = registration;
                }
                SUCCESS
            }
            "Cancel" => {
                let state = self.legacy_sockets.get_mut(&object).unwrap();
                let matches = |r: &Registration| {
                    (a1 == 0 && a2 == 0) || (r.callback.function == a1 && r.callback.context == a2)
                };
                state.connect.retain(|r| !matches(r));
                for registration in [&mut state.readable, &mut state.writeable] {
                    if registration.as_ref().is_some_and(matches) {
                        *registration = None;
                    }
                }
                SUCCESS
            }
            "Connect" => {
                // AEENet.h: callback não nulo sempre recebe a conclusão depois
                // de devolver ao laço AEE, inclusive erro de tipo/endereço/rede.
                if a3 == 0 {
                    self.legacy_sockets.get_mut(&object).unwrap().error = NET_EFAULT;
                    NET_ERROR
                } else {
                    // Não aceitar para depois descartar uma conclusão prometida.
                    // Este é um teto do emulador, não um erro SDK fictício de conexão.
                    let total = self
                        .legacy_sockets
                        .values()
                        .map(|s| s.connect.len())
                        .sum::<usize>();
                    if self.legacy_sockets[&object].connect.len() >= MAX_CONNECTS_PER_SOCKET
                        || total >= MAX_CONNECTS_TOTAL
                    {
                        return Err(CpuError(
                            "limite de conclusões pendentes de socket excedido".into(),
                        ));
                    }
                    let context = self.stack_arg(0)?;
                    let error = if kind != 0 {
                        NET_EOPNOTSUPP
                    } else if a1 == 0 || a2 as u16 == 0 {
                        NET_EINVAL
                    } else {
                        NET_ENETDOWN
                    };
                    let registration = self.socket_registration(a3, context, error)?;
                    self.legacy_sockets
                        .get_mut(&object)
                        .unwrap()
                        .connect
                        .push_back(registration.unwrap());
                    SUCCESS
                }
            }
            "Bind" => {
                let error = if kind == 0 || a1 != 0 {
                    NET_EOPNOTSUPP
                } else {
                    NET_ENETDOWN
                };
                self.legacy_sockets.get_mut(&object).unwrap().error = error;
                NET_ERROR
            }
            "Read" | "ReadV" | "Write" | "WriteV" => {
                // Não há stream conectado: nenhum buffer é lido ou alterado.
                self.legacy_sockets.get_mut(&object).unwrap().error = if kind == 0 {
                    NET_ENOTCONN
                } else {
                    NET_EOPNOTSUPP
                };
                NET_ERROR
            }
            "SendTo" | "RecvFrom" => {
                self.legacy_sockets.get_mut(&object).unwrap().error = if kind == 1 {
                    NET_ENETDOWN
                } else {
                    NET_EOPNOTSUPP
                };
                NET_ERROR
            }
            "GetPeerName" => {
                // Este método devolve o erro específico diretamente, não -1.
                let error = if kind == 0 {
                    NET_ENOTCONN
                } else {
                    NET_EOPNOTSUPP
                };
                self.legacy_sockets.get_mut(&object).unwrap().error = error;
                error
            }
            "IOCtl" => {
                let failure = match a1 {
                    0x1003 => Some(NET_ENETDOWN), // Realize exige a rede indisponível.
                    0x100b | 0x100c | 0x100d => Some(if kind == 0 {
                        NET_ENOTCONN
                    } else {
                        NET_EOPNOTSUPP
                    }), // Listen, Accept, Shutdown.
                    0x100f => Some(NET_ENOTCONN), // Close exige realização anterior.
                    0x100e => {
                        self.legacy_sockets.get_mut(&object).unwrap().error = NET_ENOTCONN;
                        return Ok(Some(NET_ENOTCONN)); // GetSockName devolve erro direto.
                    }
                    _ => None,
                };
                if let Some(error) = failure {
                    self.legacy_sockets.get_mut(&object).unwrap().error = error;
                    NET_ERROR
                } else {
                    // Não aceitar opções cuja semântica/estado ainda não existe.
                    EUNSUPPORTED
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    pub(super) fn deliver_legacy_socket_callbacks(
        &mut self,
        budget: u64,
    ) -> Result<Vec<Outcome>, CpuError> {
        // Só o lote existente na entrada: rearmar não reentra nesta volta.
        // Geração global também distingue Release+novo objeto no mesmo endereço.
        let mut ready = Vec::new();
        for (&object, state) in &self.legacy_sockets {
            for r in &state.connect {
                ready.push((object, Channel::Connect, r.epoch));
            }
            for (channel, registration) in [
                (Channel::Readable, state.readable),
                (Channel::Writeable, state.writeable),
            ] {
                if let Some(r) = registration {
                    ready.push((object, channel, r.epoch));
                }
            }
        }
        // Endereço baixo não dá prioridade: a cauda antiga precede inscrições
        // rearmadas por um callback que consumiu sua fatia de instruções.
        ready.sort_unstable_by_key(|(_, _, epoch)| *epoch);
        let mut outcomes = Vec::new();
        for (object, channel, epoch) in ready {
            let Some(state) = self.legacy_sockets.get_mut(&object) else {
                continue;
            };
            let Some(registration) = state.take_registration(channel, epoch) else {
                continue;
            };
            if matches!(channel, Channel::Connect) {
                state.error = registration.error;
            }
            let outcome = self.call_guest(
                registration.callback.function,
                [registration.callback.context, registration.error, 0, 0],
                budget,
            )?;
            let incomplete = !matches!(outcome, Outcome::Returned { .. });
            outcomes.push(outcome);
            if incomplete {
                break;
            }
        }
        Ok(outcomes)
    }

    pub(super) fn grava_legacy_sockets(&self, sections: &mut crate::save_state::Secoes) {
        sections.poe_registros(
            SECTION,
            self.legacy_sockets.iter().map(|(&object, state)| {
                let mut row = vec![object, state.kind, state.error];
                for registration in [
                    state.readable,
                    state.writeable,
                    state.connect.front().copied(),
                ] {
                    if let Some(r) = registration {
                        row.extend([1, r.callback.function, r.callback.context]);
                    } else {
                        row.extend([0; 3]);
                    }
                }
                row.push(state.connect.front().map(|r| r.error).unwrap_or(0));
                row
            }),
        );
        // A cabeça conserva os treze campos do primeiro formato. Saves antigos
        // sem esta seção opcional continuam representando zero ou uma conclusão.
        sections.poe_registros(
            QUEUE_SECTION,
            self.legacy_sockets.iter().flat_map(|(&object, state)| {
                state
                    .connect
                    .iter()
                    .skip(1)
                    .map(move |r| vec![object, r.callback.function, r.callback.context, r.error])
            }),
        );
        // Salvar a ordem lógica, não as gerações do host: os tokens novos após
        // restaurar precisam continuar distinguindo objetos liberados/recriados.
        let mut order = Vec::new();
        for (&object, state) in &self.legacy_sockets {
            for (index, registration) in state.connect.iter().enumerate() {
                order.push((registration.epoch, object, 0, index as u32));
            }
            for (channel, registration) in [(1, state.readable), (2, state.writeable)] {
                if let Some(registration) = registration {
                    order.push((registration.epoch, object, channel, 0));
                }
            }
        }
        order.sort_unstable_by_key(|(epoch, _, _, _)| *epoch);
        sections.poe_registros(
            ORDER_SECTION,
            order
                .into_iter()
                .map(|(_, object, channel, index)| vec![object, channel, index]),
        );
    }

    pub(super) fn restaura_legacy_sockets(
        &mut self,
        reader: &crate::save_state::Leitor<'_>,
    ) -> Result<(), crate::save_state::Erro> {
        let invalid = |reason: &str| crate::save_state::Erro::Secao {
            nome: SECTION.into(),
            motivo: reason.into(),
        };
        let queue_invalid = |reason: &str| crate::save_state::Erro::Secao {
            nome: QUEUE_SECTION.into(),
            motivo: reason.into(),
        };
        let order_invalid = |reason: &str| crate::save_state::Erro::Secao {
            nome: ORDER_SECTION.into(),
            motivo: reason.into(),
        };
        let tail = if let Some(bytes) = reader.secao(QUEUE_SECTION) {
            if bytes.len() < 4 || bytes.len() > 4 + MAX_CONNECTS_TOTAL * 4 * 4 {
                return Err(queue_invalid("fila fora do limite"));
            }
            let words = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
            if words > MAX_CONNECTS_TOTAL * 4 || bytes.len() != 4 + words * 4 {
                return Err(queue_invalid("contagem ou tamanho de fila inválido"));
            }
            reader.registros(QUEUE_SECTION, 4)?
        } else {
            Vec::new()
        };
        let live = self
            .objects
            .live_by_kind()
            .into_iter()
            .find_map(|(kind, count)| (kind == Interface::SocketLegacy).then_some(count))
            .unwrap_or(0);
        let order = if let Some(bytes) = reader.secao(ORDER_SECTION) {
            let max_events = MAX_CONNECTS_TOTAL + 2 * live;
            if bytes.len() < 4 || bytes.len() > 4 + max_events * 3 * 4 {
                return Err(order_invalid("ordem fora do limite"));
            }
            let words = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
            if words > max_events * 3 || bytes.len() != 4 + words * 4 {
                return Err(order_invalid("contagem ou tamanho de ordem inválido"));
            }
            Some(reader.registros(ORDER_SECTION, 3)?)
        } else {
            None
        };
        let Some(bytes) = reader.secao(SECTION) else {
            if live != 0 || !tail.is_empty() || order.as_ref().is_some_and(|rows| !rows.is_empty())
            {
                return Err(invalid("socket vivo sem estado"));
            }
            self.legacy_sockets.clear();
            return Ok(());
        };
        // Conferir o tamanho exato antes de alocar registros ou aceitar cauda extra.
        if bytes.len() != 4 + live * FIELDS * 4 {
            return Err(invalid("tamanho não corresponde aos sockets vivos"));
        }
        let rows = reader.registros(SECTION, FIELDS)?;
        if rows.len() != live {
            return Err(invalid("contagem de sockets inválida"));
        }
        let mut restored = BTreeMap::new();
        let mut epoch = self.legacy_socket_epoch;
        for row in rows {
            let object = row[0];
            if row[1] > 1
                || self.objects.kind_of(object) != Some(Interface::SocketLegacy)
                || self.objects.contagem(object) == 0
                || restored.contains_key(&object)
            {
                return Err(invalid("tipo ou proprietário de socket inválido"));
            }
            let mut registration =
                |offset: usize,
                 error: u32|
                 -> Result<Option<Registration>, crate::save_state::Erro> {
                    match row[offset] {
                        0 if row[offset + 1] == 0 && row[offset + 2] == 0 && error == 0 => Ok(None),
                        1 if row[offset + 1] != 0 => {
                            epoch = epoch
                                .checked_add(1)
                                .ok_or_else(|| invalid("gerações esgotadas"))?;
                            Ok(Some(Registration {
                                callback: Callback {
                                    function: row[offset + 1],
                                    context: row[offset + 2],
                                },
                                error,
                                epoch,
                            }))
                        }
                        _ => Err(invalid("registro de callback inválido")),
                    }
                };
            let readable = registration(3, 0)?;
            let writeable = registration(6, 0)?;
            let connect = registration(9, row[12])?;
            if connect.is_some() && !matches!(row[12], NET_ENETDOWN | NET_EOPNOTSUPP | NET_EINVAL) {
                return Err(invalid("conclusão de conexão offline inválida"));
            }
            restored.insert(
                object,
                SocketState {
                    kind: row[1],
                    error: row[2],
                    readable,
                    writeable,
                    connect: connect.into_iter().collect(),
                },
            );
        }
        let mut total = restored.values().map(|s| s.connect.len()).sum::<usize>();
        if total > MAX_CONNECTS_TOTAL {
            return Err(queue_invalid("fila excede o limite global"));
        }
        for row in tail {
            let Some(state) = restored.get_mut(&row[0]) else {
                return Err(queue_invalid("socket proprietário ausente"));
            };
            if state.connect.is_empty()
                || row[1] == 0
                || !matches!(row[3], NET_ENETDOWN | NET_EOPNOTSUPP | NET_EINVAL)
                || state.connect.len() >= MAX_CONNECTS_PER_SOCKET
                || total >= MAX_CONNECTS_TOTAL
            {
                return Err(queue_invalid("conclusão ou tamanho de fila inválido"));
            }
            epoch = epoch
                .checked_add(1)
                .ok_or_else(|| queue_invalid("gerações esgotadas"))?;
            state.connect.push_back(Registration {
                callback: Callback {
                    function: row[1],
                    context: row[2],
                },
                error: row[3],
                epoch,
            });
            total += 1;
        }
        if let Some(order) = order {
            let pending = restored
                .values()
                .map(|s| {
                    s.connect.len()
                        + usize::from(s.readable.is_some())
                        + usize::from(s.writeable.is_some())
                })
                .sum::<usize>();
            if order.len() != pending {
                return Err(order_invalid("ordem não cobre todos os callbacks"));
            }
            let mut seen = BTreeSet::new();
            let mut next_connect = BTreeMap::<u32, u32>::new();
            epoch = self.legacy_socket_epoch;
            for row in order {
                if !seen.insert((row[0], row[1], row[2])) {
                    return Err(order_invalid("callback repetido na ordem"));
                }
                let Some(state) = restored.get_mut(&row[0]) else {
                    return Err(order_invalid("socket proprietário ausente"));
                };
                let Some(registration) = state.registration_mut(row[1], row[2]) else {
                    return Err(order_invalid("canal ou índice de callback inválido"));
                };
                if row[1] == 0 {
                    let expected = next_connect.entry(row[0]).or_default();
                    if row[2] != *expected {
                        return Err(order_invalid("ordem inverte a fila Connect"));
                    }
                    *expected += 1;
                }
                epoch = epoch
                    .checked_add(1)
                    .ok_or_else(|| order_invalid("gerações esgotadas"))?;
                registration.epoch = epoch;
            }
        }
        self.legacy_sockets = restored;
        self.legacy_socket_epoch = epoch;
        Ok(())
    }
}
