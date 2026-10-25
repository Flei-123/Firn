use flsp::auth::Account;
use flsp::config::{self, LoaderType, Profile};
use flsp::engine::singleplayer::{self, settings::{GameMode, WorldSettings}};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_world_start_join_and_clean_stop() {
    let home = std::env::var("FLSP_HOME").expect("FLSP_HOME");
    std::env::set_var("HOME", &home);
    let engine = std::path::Path::new(&home).join("engine-bin");
    std::fs::create_dir_all(&engine).unwrap();
    let script = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake_mcplay.py")).unwrap();
    std::fs::write(engine.join("mcplay"), script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(engine.join("mcplay"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("FLEI_ENGINE_PATH", engine.join("mcplay"));
    let result_path = std::path::Path::new(&home).join("result.json");
    std::env::set_var("FLSP_RESULT", &result_path);

    let mc = std::env::var("FLSP_MC").unwrap_or_else(|_| "1.20.1".into());
    let profile = Profile { id: "p1".into(), name: "P".into(), minecraft_version: mc.clone(), loader: LoaderType::Fleiengine, ..Profile::default() };
    let account = Account { name: "Steve".into(), uuid: "00000000-0000-0000-0000-000000000001".into(), kind: "offline".into(), xuid: String::new() };
    let entry = singleplayer::create_world("p1", WorldSettings { name: "Test World".into(), seed: "12345".into(), mode: GameMode::Creative, ..WorldSettings::default() }).unwrap();

    let running: Arc<Mutex<HashMap<String, u32>>> = Arc::new(Mutex::new(HashMap::new()));
    let started = Instant::now();
    let pid = singleplayer::launch_singleplayer(&tauri::Window, &profile, &account, &entry.folder, running.clone()).await.unwrap();
    println!("client pid {pid} after {:?}", started.elapsed());

    let deadline = Instant::now() + Duration::from_secs(60);
    while running.lock().unwrap().contains_key("p1") && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(!running.lock().unwrap().contains_key("p1"));
    let result: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&result_path).unwrap()).unwrap();
    println!("{result}");
    assert_eq!(result["status_version"].as_str().unwrap(), mc);
    assert_eq!(result["host"], "127.0.0.1");
    assert!(matches!(result["login_packet"].as_u64(), Some(2)), "offline login success expected: {result}");
    let port = result["port"].as_u64().unwrap() as u16;

    let world_dir = singleplayer::profile_worlds_root("p1").join(&entry.folder);
    let level = world_dir.join("world").join("level.dat");
    let deadline = Instant::now() + Duration::from_secs(60);
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(300));
    }
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err(), "server still listening after client exit");
    assert!(level.is_file(), "world was not saved");
    let props = std::fs::read_to_string(world_dir.join("server.properties")).unwrap();
    assert!(props.contains("level-seed=12345") && props.contains("gamemode=creative") && props.contains("online-mode=false"));
    let listed = singleplayer::list_worlds("p1");
    assert!(listed[0].generated);
    let _ = config::base_dir();
}
