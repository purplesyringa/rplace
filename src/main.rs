#![feature(duration_constants)]

mod ejudge;
mod grid;
mod tokendb;

use anyhow::{bail, Context, Result};
use futures_util::{stream::SplitSink, SinkExt};
use rocket::{
    form::Form,
    fs::{relative, FileServer},
    futures::{StreamExt, TryStreamExt},
    routes, FromForm, State,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock};
use tokio_tungstenite::WebSocketStream;
use tungstenite::protocol::Message;

fn get_cooldown_period() -> Duration {
    return 60 * Duration::SECOND;
}

struct GlobalState {
    grid: RwLock<grid::Grid>,
    tokendb: tokendb::TokenDB,
    ws_connections: Arc<
        RwLock<HashMap<SocketAddr, Arc<Mutex<SplitSink<WebSocketStream<TcpStream>, Message>>>>>,
    >,
}

#[derive(FromForm)]
struct GetTokenForm<'r> {
    login: &'r str,
    password: &'r str,
    group: usize,
}

#[rocket::post("/get_token", data = "<info>")]
async fn get_token(state: &State<&'static GlobalState>, info: Form<GetTokenForm<'_>>) -> String {
    let check_result = match ejudge::check_account(info.login, info.password, info.group).await {
        Ok(x) => x,
        Err(e) => {
            return format!("Unexpected error: {:?}", e);
        }
    };
    if !check_result {
        return "Invalid credentials".to_string();
    }
    let token = (*state)
        .tokendb
        .create_token_for_user(&format!("ejudge/{}", info.login));
    match token {
        Ok(token) => format!("Your token: {}", token.to_string()),
        Err(e) => e.to_string(),
    }
}

async fn handle_ws_message(state: &'static GlobalState, msg: Message) -> Result<()> {
    match msg {
        Message::Text(ref s) => {
            let parts: Vec<&str> = s.split(" ").collect();
            if parts.len() != 8 || parts[0] != "set" {
                bail!("Invalid command syntax: must be 'set <token> <x> <y> <r> <g> <b> <a>'");
            }

            let token = tokendb::Token::try_from_string(&parts[1])?;

            let mut nums = [0usize; 6];
            for i in 0..6 {
                match parts[2 + i].parse() {
                    Ok(num) => nums[i] = num,
                    Err(e) => {
                        bail!("Invalid command syntax: not a number: {}", e);
                    }
                }
            }
            if nums[2].max(nums[3]).max(nums[4]).max(nums[5]) > 255 {
                bail!(
                    "Invalid command syntax: color components must be in range 0..255 (inclusive)"
                );
            }

            state.tokendb.try_use_token(token, get_cooldown_period())?;

            let x: usize = nums[0];
            let y: usize = nums[1];
            let r: u8 = nums[2] as u8;
            let g: u8 = nums[3] as u8;
            let b: u8 = nums[4] as u8;
            let a: u8 = nums[5] as u8;

            state
                .grid
                .write()
                .await
                .set_cell(x, y, grid::CellData { r, g, b, a })?;

            for (_, ws) in state.ws_connections.read().await.iter() {
                let ws = ws.clone();
                tokio::spawn(async move {
                    ws.lock()
                        .await
                        .send(Message::Text(format!(
                            "set {} {} {} {} {} {}",
                            x, y, r, g, b, a
                        )))
                        .await
                });
            }

            Ok(())
        }
        _ => {
            bail!("Invalid message: must be text");
        }
    }
}

async fn handle_ws_connection(
    state: &'static GlobalState,
    raw_stream: TcpStream,
    addr: SocketAddr,
) -> Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .context("Handshake failed")?;

    println!("WS connection from {}", addr);

    let (mut outgoing, mut incoming) = ws_stream.split();

    let grid = state.grid.read().await;
    let grid_width = grid.width();
    let grid_height = grid.height();
    let grid_data = grid.get_data_serialized();
    drop(grid);

    outgoing
        .send(Message::Text(format!(
            "grid {} {}",
            grid_width, grid_height
        )))
        .await
        .context("Failed to send initial grid data")?;
    outgoing
        .send(Message::Binary(grid_data))
        .await
        .context("Failed to send initial grid data")?;

    let outgoing = Arc::new(Mutex::new(outgoing));

    state
        .ws_connections
        .write()
        .await
        .insert(addr, outgoing.clone());

    while let Some(msg) = incoming.try_next().await? {
        if let Err(e) = handle_ws_message(state, msg).await {
            eprintln!("{}", e);
            outgoing
                .lock()
                .await
                .send(Message::Text(format!("error {}", e)))
                .await?;
        }
    }

    println!("{} disconnected", &addr);

    Ok(())
}

async fn start_http_server(state: &'static GlobalState) -> Result<()> {
    rocket::build()
        .mount("/", routes![get_token])
        .mount("/", FileServer::from(relative!("static")))
        .manage(state)
        .launch()
        .await?;
    Ok(())
}

async fn start_ws_server(state: &'static GlobalState) {
    async fn go(state: &'static GlobalState) -> Result<()> {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:9000")
            .await
            .context("Failed to bind to 0.0.0.0:9000")?;
        while let Ok((stream, addr)) = listener.accept().await {
            tokio::spawn(async move {
                if let Err(e) = handle_ws_connection(state, stream, addr).await {
                    eprintln!("Websocket error: {:?}", e);
                }
            });
        }
        Ok(())
    }

    match go(state).await {
        Ok(()) => {
            panic!("WS server stopped");
        }
        Err(e) => {
            eprintln!("{:?}", e);
            panic!("WS server crashed");
        }
    }
}

enum Command {
    Init(String, u32, u32),
    Serve(String),
}

fn get_command() -> Result<Command> {
    let mut args = std::env::args();
    args.next().unwrap();

    let command = args
        .next()
        .context("The first CLI argument must be the command name: 'serve' or 'init'")?;

    match command.as_ref() {
        "init" => {
            let dir_path = args.next().context("'rplace init' expects the path to the directory for permanent storage as the first argument")?;
            let width: u32 = args
                .next()
                .context("'rplace init' expects the width of the grid as the second argument")?
                .parse()
                .context("Invalid width")?;
            let height: u32 = args
                .next()
                .context("'rplace init' expects the height of the grid as the third argument")?
                .parse()
                .context("Invalid height")?;
            Ok(Command::Init(dir_path, width, height))
        }
        "serve" => {
            let dir_path = args.next().context("'rplace serve' expects the path to the directory for permanent storage as an argument")?;
            Ok(Command::Serve(dir_path))
        }
        _ => bail!(
            "Unknown CLI command: {}. 'serve' and 'init' are valid",
            command
        ),
    }
}

#[rocket::main]
async fn main() -> Result<()> {
    match get_command()? {
        Command::Init(dir_path, width, height) => {
            std::fs::create_dir(&dir_path)
                .context("Failed to create the permanent storage directory")?;

            grid::Grid::create_file(format!("{}/grid", dir_path).as_ref(), width, height)
                .context("Failed to create grid data file")?;

            println!("Created a storage at {}", dir_path);
            Ok(())
        }
        Command::Serve(dir_path) => {
            let grid_data_file = std::fs::File::options()
                .read(true)
                .write(true)
                .open(format!("{}/grid", dir_path))
                .context("Failed to open grid data file")?;
            let grid =
                grid::Grid::from_file(&grid_data_file).context("Failed to load grid data file")?;

            let tokendb = tokendb::TokenDB::open(format!("{}/tokendb", dir_path).as_ref())
                .context("Failed to load tokendb file")?;

            println!("Loaded grid of size {} x {}", grid.width(), grid.height());

            let state = Box::leak(Box::new(GlobalState {
                grid: RwLock::new(grid),
                tokendb,
                ws_connections: Arc::new(RwLock::new(HashMap::new())),
            }));

            tokio::spawn(start_ws_server(state));
            start_http_server(state).await?;
            Ok(())
        }
    }
}
