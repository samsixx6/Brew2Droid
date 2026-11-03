//! Agenda virtual vazia do BREW MP, sem acesso à agenda do anfitrião.
use super::*;

const BAD_PARAMETER: u32 = 14;
const BAD_ITEM: u32 = 16;
const UNSUPPORTED: u32 = 20;
const NO_MORE: u32 = 47;
const CONTACTS_IID: u32 = 0x0107_163b;
const RESULTS_IID: u32 = 0x0107_000a;
const IQI_IID: u32 = 0x0100_0001;
const MAX_FILTER: usize = 4096;

#[derive(Debug, PartialEq)]
enum Token {
    Word(String),
    Literal(String),
    Number,
    Operator,
}

fn tokens(filter: &str) -> Result<Vec<Token>, u32> {
    let chars: Vec<char> = filter.chars().collect();
    let mut at = 0;
    let mut out = Vec::new();
    while at < chars.len() {
        let c = chars[at];
        if c.is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if c == '\'' {
            at += 1;
            let mut text = String::new();
            let mut ended = false;
            while at < chars.len() {
                let c = chars[at];
                at += 1;
                if c == '\'' {
                    if chars.get(at) == Some(&'\'') {
                        text.push('\'');
                        at += 1;
                    } else {
                        ended = true;
                        break;
                    }
                } else {
                    text.push(c);
                }
            }
            if !ended {
                return Err(BAD_PARAMETER);
            }
            out.push(Token::Literal(text));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = at;
            while at < chars.len() && (chars[at].is_ascii_alphanumeric() || chars[at] == '_') {
                at += 1;
            }
            out.push(Token::Word(
                chars[start..at]
                    .iter()
                    .collect::<String>()
                    .to_ascii_uppercase(),
            ));
        } else if c.is_ascii_digit() || matches!(c, '+' | '-') {
            if matches!(c, '+' | '-') {
                at += 1;
            }
            let digits = at;
            while at < chars.len() && chars[at].is_ascii_digit() {
                at += 1;
            }
            if digits == at {
                return Err(BAD_PARAMETER);
            }
            if chars.get(at) == Some(&'.') {
                at += 1;
                let digits = at;
                while at < chars.len() && chars[at].is_ascii_digit() {
                    at += 1;
                }
                if digits == at {
                    return Err(BAD_PARAMETER);
                }
            }
            out.push(Token::Number);
        } else if matches!(c, '<' | '>' | '=' | '!') {
            at += 1;
            let next = chars.get(at).copied();
            if next == Some('=') || (c == '<' && next == Some('>')) {
                at += 1;
            } else if c == '!' {
                return Err(BAD_PARAMETER);
            }
            out.push(Token::Operator);
        } else {
            return Err(BAD_PARAMETER);
        }
    }
    Ok(out)
}

fn word(tokens: &[Token], at: usize, value: &str) -> bool {
    matches!(tokens.get(at), Some(Token::Word(text)) if text == value)
}

fn field(tokens: &[Token], at: &mut usize, ordering: bool) -> Result<(), u32> {
    let Some(Token::Word(name)) = tokens.get(*at) else {
        return Err(BAD_PARAMETER);
    };
    // São os nomes de busca do SDK, não nomes de pessoas ou campos inventados.
    let supported = matches!(
        name.as_str(),
        "LASTNAME"
            | "FIRSTNAME"
            | "MIDDLENAME"
            | "NICKNAME"
            | "PREFIX"
            | "SUFFIX"
            | "NAME"
            | "NUMBER"
            | "ADDRLINE1"
            | "ADDRSTREET"
            | "ADDRCITY"
            | "ADDRSTATE"
            | "ADDRZIPCODE"
            | "ADDRCOUNTRY"
            | "ADDRNOTES"
            | "BIRTHDAY"
            | "NOTES"
            | "GENDER"
            | "ORGANIZATION"
            | "JOBTITLE"
            | "DEPARTMENT"
            | "ICON"
            | "PHOTO"
            | "RINGTONE"
            | "VIDEORING"
            | "GROUPNAME"
            | "SPEEDDIAL"
            | "EMAIL"
            | "CHANGECOUNTER"
            | "FULLNAME"
            | "OTHERNAME"
            | "HOMENUMBER"
            | "CELLNUMBER"
            | "WORKNUMBER"
            | "PAGERNUMBER"
            | "VOICENUMBER"
            | "FAXNUMBER"
            | "PREFNUMBER"
            | "BUSINESSFAXNUMBER"
            | "GENERICNUMBER"
            | "OTHERNUMBER"
            | "IMUSERID"
            | "IMSERVICENAME"
    ) || (ordering && matches!(name.as_str(), "LASTFIRSTNAME" | "FIRSTLASTNAME"));
    if !supported {
        return Err(UNSUPPORTED);
    }
    *at += 1;
    Ok(())
}

fn valid_filter(filter: &str) -> Result<(), u32> {
    let tokens = tokens(filter)?;
    let mut at = 0;
    if tokens.is_empty() {
        return Ok(());
    }
    if !word(&tokens, at, "ORDERBY") {
        loop {
            field(&tokens, &mut at, false)?;
            if word(&tokens, at, "ISNULL") || word(&tokens, at, "ISNOTNULL") {
                at += 1;
            } else if word(&tokens, at, "LIKE") {
                at += 1;
                if !matches!(tokens.get(at), Some(Token::Literal(_))) {
                    return Err(BAD_PARAMETER);
                }
                at += 1;
                if word(&tokens, at, "ESCAPE") {
                    at += 1;
                    if !matches!(tokens.get(at), Some(Token::Literal(s)) if s.chars().count() == 1)
                    {
                        return Err(BAD_PARAMETER);
                    }
                    at += 1;
                }
            } else {
                if !matches!(tokens.get(at), Some(Token::Operator)) {
                    return Err(BAD_PARAMETER);
                }
                at += 1;
                if !matches!(tokens.get(at), Some(Token::Number | Token::Literal(_))) {
                    return Err(BAD_PARAMETER);
                }
                at += 1;
            }
            if !word(&tokens, at, "AND") {
                break;
            }
            at += 1;
        }
    }
    if word(&tokens, at, "ORDERBY") {
        at += 1;
        field(&tokens, &mut at, true)?;
        if word(&tokens, at, "ASC") || word(&tokens, at, "DESC") {
            at += 1;
        }
    }
    if at == tokens.len() {
        Ok(())
    } else {
        Err(BAD_PARAMETER)
    }
}

impl<C: CpuBackend> Machine<C> {
    pub(super) fn create_pim_contacts(&mut self, out: u32) -> Result<u32, CpuError> {
        if !self.contacts_null_output(out) {
            return Ok(BAD_PARAMETER);
        }
        let contacts = self.new_object(Interface::PimContacts)?;
        if contacts == 0 {
            return Ok(2); // AEE_ENOMEMORY do SDK moderno.
        }
        if self.cpu.write_u32(out, contacts).is_err() {
            self.objects.release(contacts);
            return Ok(BAD_PARAMETER);
        }
        Ok(SUCCESS)
    }

    fn contacts_filter(&self, address: u32) -> Result<(), u32> {
        if address == 0 {
            return Err(BAD_PARAMETER);
        }
        let mut units = Vec::new();
        for index in 0..MAX_FILTER {
            let at = address.checked_add(index as u32 * 2).ok_or(BAD_PARAMETER)?;
            let mut bytes = [0; 2];
            self.cpu
                .read_mem(at, &mut bytes)
                .map_err(|_| BAD_PARAMETER)?;
            let unit = u16::from_le_bytes(bytes);
            if unit == 0 {
                let text = String::from_utf16(&units).map_err(|_| BAD_PARAMETER)?;
                return valid_filter(&text);
            }
            units.push(unit);
        }
        Err(BAD_PARAMETER)
    }

    fn contacts_null_output(&mut self, address: u32) -> bool {
        address != 0 && self.cpu.write_u32(address, 0).is_ok()
    }

    pub(super) fn pim_contacts_call(
        &mut self,
        iface: Interface,
        slot: u32,
    ) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        if self.objects.kind_of(this) != Some(iface) {
            return Ok(Some(BAD_PARAMETER));
        }
        let result = match slot {
            0 => self.objects.add_ref(this),
            1 => self.objects.release(this),
            2 => {
                let iid = self.arg(1);
                let out = self.arg(2);
                if !self.contacts_null_output(out) {
                    return Ok(Some(BAD_PARAMETER));
                }
                let own_iid = if iface == Interface::PimContacts {
                    CONTACTS_IID
                } else {
                    RESULTS_IID
                };
                if iid == own_iid || iid == IQI_IID {
                    self.cpu.write_u32(out, this)?;
                    self.objects.add_ref(this);
                    SUCCESS
                } else {
                    3
                } // AEE_ECLASSNOTSUPPORT do Brew MP.
            }
            3 if iface == Interface::PimContactsResults => {
                if !self.contacts_null_output(self.arg(1)) {
                    BAD_PARAMETER
                } else {
                    NO_MORE
                }
            }
            4 if iface == Interface::PimContactsResults => SUCCESS,
            _ if iface == Interface::PimContactsResults => return Ok(None),
            3 | 6 => {
                let out = self.arg(if slot == 3 { 1 } else { 2 });
                if !self.contacts_null_output(out) {
                    BAD_PARAMETER
                } else if slot == 6 {
                    BAD_ITEM
                } else {
                    UNSUPPORTED
                }
            }
            7 => BAD_ITEM,
            20 | 21 => {
                let filter = self.arg(1);
                let out = self.arg(2);
                if slot == 20 && !self.contacts_null_output(out) {
                    return Ok(Some(BAD_PARAMETER));
                }
                if let Err(error) = self.contacts_filter(filter) {
                    return Ok(Some(error));
                }
                if slot == 21 {
                    if !self.contacts_null_output(out) {
                        BAD_PARAMETER
                    } else {
                        SUCCESS
                    }
                } else {
                    let results = self.new_object(Interface::PimContactsResults)?;
                    if results == 0 {
                        2
                    }
                    // AEE_ENOMEMORY, não uma enumeração falsa.
                    else if self.cpu.write_u32(out, results).is_err() {
                        self.objects.release(results);
                        BAD_PARAMETER
                    } else {
                        SUCCESS
                    }
                }
            }
            25 => {
                // Não há alterações externas nem escrita local modelada: registrar
                // notificações com sucesso fingiria um serviço que não entregamos.
                if !self.contacts_null_output(self.arg(3)) {
                    BAD_PARAMETER
                } else {
                    UNSUPPORTED
                }
            }
            26 => {
                let out = self.arg(1);
                if out == 0 || self.cpu.write_mem(out, &[0; 8]).is_err() {
                    BAD_PARAMETER
                } else {
                    SUCCESS
                }
            }
            // A API não recebe uma capacidade física fictícia nem contatos do host.
            // O restante da vtable existe com a recusa pública explícita, incluindo
            // escritas, transações e consulta de esquema/limites não modelados.
            4 | 5 | 8..=19 | 22..=24 | 27 | 28 => UNSUPPORTED,
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
}
