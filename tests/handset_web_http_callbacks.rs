#![allow(dead_code)]
include!("common/modern_ui_fixture.rs");

use std::{
    io::{Read as _, Write as _},
    net::TcpListener,
    sync::{Arc, atomic::{AtomicBool, Ordering}},
    thread::JoinHandle,
    time::{Duration, Instant},
};

const RESPONSE: u32 = OUT + 64;
const PCB: u32 = OUT + 256;
const FINAL_COUNT: u32 = OUT + 288;
const URL: u32 = OUT + 320;
const OPTIONS: u32 = OUT + 1024;
const HANDLER_DATA: u32 = OUT + 2048;

struct Server {
    address: std::net::SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Vec<Vec<u8>>>>,
}

impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = std::thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(12);
            let mut requests = Vec::new();
            while !stopped.load(Ordering::Acquire) && Instant::now() < end {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        // No Windows, accept pode herdar o modo não bloqueante do
                        // listener; os timeouts abaixo exigem leitura bloqueante.
                        socket.set_nonblocking(false).unwrap();
                        socket.set_read_timeout(Some(Duration::from_millis(250))).unwrap();
                        socket.set_write_timeout(Some(Duration::from_millis(250))).unwrap();
                        let mut request = Vec::new();
                        let mut byte = [0];
                        while request.len() < 65536 && !request.ends_with(b"\r\n\r\n") {
                            if socket.read_exact(&mut byte).is_err() { break; }
                            request.push(byte[0]);
                        }
                        let status = if request.starts_with(b"GET /new ") { "202 Accepted" } else { "200 OK" };
                        let response = format!("HTTP/1.1 {status}\r\nX-Token: Budget-safe\r\nContent-Length: 2\r\n\r\nok");
                        // Cancel/Release pode fechar o socket antes do envio do servidor.
                        let _ = socket.write_all(response.as_bytes());
                        requests.push(request);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("servidor local: {error}"),
                }
            }
            requests
        });
        Self { address, stop, worker: Some(worker) }
    }

    fn url(&self, path: &str) -> String { format!("http://{}{path}", self.address) }

    fn finish(mut self) -> Vec<Vec<u8>> {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() { worker.join().unwrap(); }
    }
}

fn code(f: &mut F, offset: u32, words: &[u32]) -> u32 {
    let function = loader::MODULE_BASE + offset;
    f.m.cpu_mut().write_mem(function, &words.iter().flat_map(|word| word.to_le_bytes()).collect::<Vec<_>>()).unwrap();
    function
}

fn setup(f: &mut F, url: &str) -> u32 {
    f.m.set_network(true);
    let web = f.create(0x01005000);
    let callback = code(f, 0x100, &[0xe5901000, 0xe2811001, 0xe5801000, 0xe12fff1e]);
    f.m.cpu_mut().write_mem(PCB, &[0; 24]).unwrap();
    f.put(PCB + 16, callback);
    f.put(PCB + 20, FINAL_COUNT);
    f.put(FINAL_COUNT, 0);
    f.put(RESPONSE, 0);
    f.m.cpu_mut().write_mem(HANDLER_DATA, &[0; 32]).unwrap();
    f.m.cpu_mut().write_mem(URL, format!("{url}\0").as_bytes()).unwrap();
    web
}

fn begin(f: &mut F, web: u32, pairs: &[(u32, u32)]) {
    for (index, &(id, value)) in pairs.iter().enumerate() {
        f.put(OPTIONS + index as u32 * 8, id);
        f.put(OPTIONS + index as u32 * 8 + 4, value);
    }
    f.put(OPTIONS + pairs.len() as u32 * 8, 0);
    f.extra(web, 7, [RESPONSE, PCB, URL], &[OPTIONS]);
}

fn turn(f: &mut F, budget: u64) -> bool {
    let mut yielded = false;
    for stage in 0..3 {
        let outcomes = match stage {
            0 => f.m.advance(budget).unwrap(),
            1 => f.m.deliver_signals(budget).unwrap(),
            _ => f.m.deliver_callbacks(budget).unwrap(),
        };
        for outcome in outcomes {
            match outcome {
                Outcome::Returned { .. } => {}
                Outcome::Budget => yielded = true,
                other => panic!("callback HTTP: {other:?}"),
            }
        }
        if yielded { break; }
    }
    yielded
}

#[track_caller]
fn until(f: &mut F, predicate: impl Fn(&F) -> bool, budget: u64) -> bool {
    // A suíte também roda junto de compilações; dar folga de tempo real sem
    // alterar o orçamento guest nem retirar as verificações da continuação.
    let end = Instant::now() + Duration::from_secs(8);
    let mut yielded = false;
    while !predicate(f) && Instant::now() < end {
        yielded |= turn(f, budget);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(predicate(f),
        "callback não chegou: data={:?} final={} response={:#x} pc={:#x} idle={} ready={:?}",
        (0..8).map(|word| f.read(HANDLER_DATA + word * 4)).collect::<Vec<_>>(),
        f.read(FINAL_COUNT), f.read(RESPONSE), f.m.cpu().read_reg(Reg::Pc),
        f.m.is_idle(), f.m.next_guest_work_delay());
    yielded
}

fn heap_live(f: &F) -> Vec<(u32, u32)> {
    let state = f.m.grava_estado();
    let mut allocations = zeebx::save_state::Leitor::abre(&state).unwrap().pares("heap.live").unwrap();
    allocations.sort_unstable();
    allocations
}

#[test]
fn status_handler_cancel_retires_headers_and_final_notification() {
    let server = Server::new();
    let mut f = F::new();
    let web = setup(&mut f, &server.url("/cancel"));
    // O próprio handler chama CALLBACK_Cancel(pcb), preservando o LR do callback.
    let status = code(&mut f, 0x180, &[
        0xe92d4010, 0xe1a04000, 0xe5941000, 0xe2811001, 0xe5841000,
        0xe5940004, 0xe5903008, 0xe12fff33, 0xe8bd8010,
    ]);
    let header = code(&mut f, 0x200, &[0xe5901008, 0xe2811001, 0xe5801008, 0xe12fff1e]);
    f.put(HANDLER_DATA + 4, PCB);
    begin(&mut f, web, &[(0x20003, status), (0x20004, header), (0x20002, HANDLER_DATA)]);
    assert_eq!(f.read(HANDLER_DATA), 0, "GetResponse não reentra no handler");
    until(&mut f, |f| f.read(HANDLER_DATA) != 0, 100000);
    for _ in 0..12 { turn(&mut f, 100000); }
    assert_eq!(f.read(HANDLER_DATA), 1, "cancelamento aposenta os demais status");
    assert_eq!(f.read(HANDLER_DATA + 8), 0, "headers cancelados não tocam handlerdata");
    assert_eq!(f.read(FINAL_COUNT), 0);
    assert_eq!(f.read(RESPONSE), 0);
    assert_eq!(f.call(web, 1, [0; 3]), 0);
    server.finish();
}

#[test]
fn budget_header_resumes_once_with_getline_and_releases_saved_scratch() {
    let server = Server::new();
    let mut f = F::new();
    let web = setup(&mut f, &server.url("/budget"));
    let baseline = heap_live(&f);
    // A primeira entrega conserva nome/GetLine e espera um gate do teste. Depois
    // da retomada ela ainda lê os dados; o segundo header retorna imediatamente.
    let header = code(&mut f, 0x280, &[
        0xe92d4010, 0xe1a04000, 0xe5943000, 0xe2833001, 0xe5843000,
        0xe3530001, 0x1a00000d, 0xe5841004, 0xe5842008,
        0xe594300c, 0xe3530000, 0x0afffffc,
        0xe5941004, 0xe5d11000, 0xe5841010, 0xe5942008,
        0xe5921004, 0xe5841014, 0xe5922000, 0xe5d21000,
        0xe5841018, 0xe8bd8010,
    ]);
    begin(&mut f, web, &[(0x20004, header), (0x20002, HANDLER_DATA)]);
    assert!(until(&mut f, |f| f.read(HANDLER_DATA) == 1, 64), "handler deve ceder por Budget");
    assert_eq!(f.read(FINAL_COUNT), 0, "a notificação aguarda o retorno real do handler");
    assert_eq!(f.read(RESPONSE), 0);
    let saved = f.m.grava_estado();
    f.m.restaura_estado(&saved).unwrap();
    f.put(HANDLER_DATA + 12, 1);
    until(&mut f, |f| f.read(FINAL_COUNT) == 1, 100000);
    assert_eq!(f.read(HANDLER_DATA), 2, "retomar Budget não chama a entrada do handler novamente");
    assert_eq!(f.read(HANDLER_DATA + 16), b'X' as u32);
    assert_eq!(f.read(HANDLER_DATA + 20), b"Budget-safe".len() as u32);
    assert_eq!(f.read(HANDLER_DATA + 24), b'B' as u32);
    let response = f.read(RESPONSE);
    assert_ne!(response, 0);
    let info = f.call(response, 6, [0; 3]);
    assert_eq!(f.read(info), 200, "resposta já recebida sobrevive ao save do handler");
    assert_eq!(f.call(response, 1, [0; 3]), 0);
    assert_eq!(heap_live(&f), baseline, "Release libera também scratch/pins restaurados");
    for _ in 0..4 { turn(&mut f, 100000); }
    assert_eq!(f.read(FINAL_COUNT), 1);
    f.call(web, 1, [0; 3]);
    let requests = server.finish();
    assert_eq!(requests.len(), 1, "restauração nunca repete o pedido HTTP");
}

#[test]
fn status_handler_release_allows_reused_web_and_pcb_without_old_callbacks() {
    let server = Server::new();
    let mut f = F::new();
    let web = setup(&mut f, &server.url("/old"));
    let baseline = heap_live(&f);
    let status = code(&mut f, 0x380, &[
        0xe92d4010, 0xe1a04000, 0xe5941000, 0xe2811001, 0xe5841000,
        0xe5940004, 0xe5903000, 0xe5933004, 0xe12fff33, 0xe8bd8010,
    ]);
    let header = code(&mut f, 0x400, &[0xe5901008, 0xe2811001, 0xe5801008, 0xe12fff1e]);
    f.put(HANDLER_DATA + 4, web);
    begin(&mut f, web, &[(0x20003, status), (0x20004, header), (0x20002, HANDLER_DATA)]);
    until(&mut f, |f| f.read(HANDLER_DATA) != 0, 100000);
    assert_eq!(f.read(FINAL_COUNT), 0);
    assert_eq!(f.read(RESPONSE), 0);
    // O transporte pode já ter criado Source quando STARTING é entregue. Reter
    // os poucos slots liberados evita depender da ordem de destruição deles.
    let mut fillers = Vec::new();
    let mut replacement = 0;
    for _ in 0..4 {
        let object = f.create(0x01005000);
        if object == web { replacement = object; break; }
        fillers.push(object);
    }
    assert_eq!(replacement, web, "a regressão exerce reutilização do mesmo slot");
    f.m.cpu_mut().write_mem(URL, format!("{}\0", server.url("/new")).as_bytes()).unwrap();
    begin(&mut f, replacement, &[]);
    until(&mut f, |f| f.read(FINAL_COUNT) == 1, 100000);
    let response = f.read(RESPONSE);
    let info = f.call(response, 6, [0; 3]);
    assert_eq!(f.read(info), 202, "só a transação nova publica seu resultado");
    for _ in 0..8 { turn(&mut f, 100000); }
    assert_eq!(f.read(HANDLER_DATA), 1);
    assert_eq!(f.read(HANDLER_DATA + 8), 0);
    assert_eq!(f.read(FINAL_COUNT), 1);
    f.call(response, 1, [0; 3]);
    assert_eq!(heap_live(&f), baseline);
    f.call(replacement, 1, [0; 3]);
    for filler in fillers { f.call(filler, 1, [0; 3]); }
    let requests = server.finish();
    assert_eq!(requests.iter().filter(|request| request.starts_with(b"GET /new ")).count(), 1);
}
