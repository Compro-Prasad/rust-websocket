//! Simple echo websocket server.
//! Open `http://localhost:8080/ws/index.html` in browser
//! or [python console client](https://github.com/actix/examples/blob/master/websocket/websocket-client.py)
//! could be used for testing.
#[macro_use]
extern crate lazy_static;

use awc::http::StatusCode;
use std::io;
use std::io::prelude::*;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_files as fs;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_actors::ws;

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

lazy_static! {
    static ref WHITE_IP_LIST: Mutex<Vec<String>> = Mutex::new(vec!["127.0.0.1".to_string()]);
}

fn check_ip(ip: &str) -> bool {
    println!("Checking {}", ip);
    WHITE_IP_LIST.lock().unwrap().iter().any(|x| x == ip)
}

fn add_ip(ip: &str) {
    match WHITE_IP_LIST.lock().unwrap().iter().position(|x| x == ip) {
        Some(_) => {
            println!("IP already registered");
            return;
        }
        None => {
            println!("Whitelisting {}", ip);
        }
    }
    WHITE_IP_LIST.lock().unwrap().push(ip.to_string());
}

fn remove_ip(ip: &str) {
    let index = match WHITE_IP_LIST.lock().unwrap().iter().position(|x| x == ip) {
        Some(i) => i,
        None => {
            println!("You didn't register the IP");
            return;
        }
    };
    println!("Blacklisting {}", ip);
    WHITE_IP_LIST.lock().unwrap().remove(index);
}

/// do websocket handshake and start `MyWebSocket` actor
fn ws_index(r: HttpRequest, stream: web::Payload) -> impl Responder {
    let conn_info = r.connection_info();
    let addr: Vec<&str> = conn_info.remote().unwrap().split(':').collect();
    let ip = addr[0];
    if check_ip(ip) {
        println!("Accepting request from {}", ip);
        ws::start(MyWebSocket::new(), &r, stream)
    } else {
        println!("Canceling request from {}", ip);
        Ok(HttpResponse::new(StatusCode::from_u16(403).unwrap()))
    }
}

/// websocket connection is long running connection, it easier
/// to handle with an actor
#[derive(Debug)]
struct MyWebSocket {
    /// Client must send ping at least once per 10 seconds (CLIENT_TIMEOUT),
    /// otherwise we drop connection.
    hb: Instant,
}

impl Actor for MyWebSocket {
    type Context = ws::WebsocketContext<Self>;

    /// Method is called on actor start. We start the heartbeat process here.
    fn started(&mut self, ctx: &mut Self::Context) {
        self.hb(ctx);
    }
}

/// Handler for `ws::Message`
impl StreamHandler<ws::Message, ws::ProtocolError> for MyWebSocket {
    fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
        // process websocket messages
        println!("WS: {:?}", msg);
        match msg {
            ws::Message::Ping(msg) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            ws::Message::Pong(_) => {
                self.hb = Instant::now();
            }
            ws::Message::Text(text) => ctx.text(text),
            ws::Message::Binary(bin) => ctx.binary(bin),
            ws::Message::Close(_) => {
                ctx.stop();
            }
            ws::Message::Nop => (),
        }
    }
}

impl MyWebSocket {
    fn new() -> Self {
        Self { hb: Instant::now() }
    }

    /// helper method that sends ping to client every second.
    ///
    /// also this method checks heartbeats from client
    fn hb(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                // heartbeat timed out
                println!("Websocket Client heartbeat failed, disconnecting!");

                // stop actor
                ctx.stop();

                // don't try to send a ping
                return;
            }

            ctx.ping("");
        });
    }
}

fn ip_manager() {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let cmd_args: Vec<&str> = line.split(' ').collect();
        if cmd_args[0] == "block" {
            remove_ip(cmd_args[1]);
        } else if cmd_args[0] == "allow" {
            add_ip(cmd_args[1]);
        } else if cmd_args[0] == "list" {
            // We could write this information to a file. But that is trivial
            // enough and unnecessary for a test. Hope you understand :)
            println!("{:#?}", WHITE_IP_LIST.lock().unwrap());
        } else {
            println!("Unknown Command. Try:");
            println!("    block ip");
            println!("    allow ip");
            println!("    list   - List all whitelisted ips");
            println!("    Ctrl+D - Quit");
        }
    }
}

fn main() {
    std::env::set_var("RUST_LOG", "actix_server=info,actix_web=info");
    env_logger::init();

    thread::spawn(move || {
        // The main thread will not wait for this thread to finish. That
        // might mean that the next println isn't even executed before the
        // program exits.
        println!("Hello from server thread");
        HttpServer::new(|| {
            App::new()
                // enable logger
                //.wrap(middleware::Logger::default())
                // websocket route
                .service(web::resource("/ws/").route(web::get().to(ws_index)))
                // static files
                .service(fs::Files::new("/", "static/").index_file("index.html"))
        })
        // start http server on 127.0.0.1:8080
        .bind("127.0.0.1:8080")?
        .run()
    });
    ip_manager();
}
