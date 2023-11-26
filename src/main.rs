use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};

type SharedClientMap = Arc<Mutex<HashMap<SocketAddr, broadcast::Sender<String>>>>;

#[tokio::main]
async fn main() {
    println!("Starting server...");
    let listener = TcpListener::bind("localhost:12345")
        .await
        .expect("Failed to bind to port");
    let client_map: SharedClientMap = Arc::new(Mutex::new(HashMap::new()));
    let (tx, _rx) = broadcast::channel(10);

    loop {
        let (socket, addr) = listener.accept().await.unwrap();
        let client_map = client_map.clone();
        let tx = tx.clone();

        client_map.lock().await.insert(addr, tx.clone());
        println!("Client {} connected", addr);

        tokio::spawn(async move {
            handle_client(socket, addr, client_map, tx).await;
        });
    }
}

async fn handle_client(
    mut socket: TcpStream,
    addr: SocketAddr,
    client_map: SharedClientMap,
    tx: broadcast::Sender<String>,
) {
    let mut rx = tx.subscribe();
    let mut buffer = [0; 1024];

    loop {
        tokio::select! {
            result = socket.read(&mut buffer) => {
                let bytes_read = match result {
                    Ok(0) => {
                        // Client disconnected, clean up.
                        remove_client(&addr, &client_map).await;
                        return;
                    },
                    Ok(bytes) => bytes,
                    Err(e) => {
                        eprintln!("Failed to read from socket; addr = {:?}, error = {:?}", addr, e);
                        remove_client(&addr, &client_map).await;
                        return;
                    }
                };

                let msg = String::from_utf8_lossy(&buffer[..bytes_read]);
                println!("Received message from {}: {}", addr, msg);
                broadcast_message(&msg, addr, &tx).await;
            }
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        let _ = send_message_to_client(&mut socket, &msg).await;
                    }
                    Err(e) => {
                        eprintln!("Broadcast channel error; addr = {:?}, error = {:?}", addr, e);
                        break;
                    }
                }
            }
        }
    }
}

async fn broadcast_message(message: &str, sender_addr: SocketAddr, tx: &broadcast::Sender<String>) {
    let _ = tx.send(format!("{}: {}", sender_addr, message));
}

async fn send_message_to_client(
    socket: &mut TcpStream,
    message: &str,
) -> Result<(), std::io::Error> {
    socket.write_all(message.as_bytes()).await
}

async fn remove_client(addr: &SocketAddr, client_map: &SharedClientMap) {
    let mut clients = client_map.lock().await;
    clients.remove(addr);
    println!("Client {} disconnected", addr);
}
