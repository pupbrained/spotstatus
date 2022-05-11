use {
    dotenv::dotenv,
    rocket::{
        get,
        http::{ContentType, Status},
        launch, routes, State,
    },
    rspotify::{
        clients::{BaseClient, OAuthClient},
        model::{CurrentlyPlayingContext, FullTrack, PlayableItem},
        scopes, AuthCodeSpotify, Credentials, OAuth, Token,
    },
    std::env,
    tokio::sync::watch::{self, Receiver, Sender},
};

fn send_to_endpoint(tx: Sender<String>, spotify: AuthCodeSpotify) {
    tokio::spawn(async move {
        loop {
            let val = match spotify.current_user_playing_item().await {
                Ok(val) => match val {
                    Some(CurrentlyPlayingContext {
                        item: Some(PlayableItem::Track(FullTrack { artists, name, .. })),
                        ..
                    }) => format!(
                        "{} - {}",
                        artists
                            .iter()
                            .map(|x| x.clone().name)
                            .collect::<Vec<_>>()
                            .join(", "),
                        name
                    ),
                    None => "No song playing".to_string(),
                    _ => "Unknown".to_string(),
                },
                Err(e) => format!("Error! {}", e),
            };

            tx.send(val).expect("Failed to update value");
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    });
}

#[get("/nowplaying/song")]
async fn get_song(rx: &State<Receiver<String>>) -> (Status, (ContentType, String)) {
    (
        Status::Ok,
        (ContentType::JSON, format!("\"{}\"", *rx.borrow())),
    )
}

fn refresh_token(spotify: AuthCodeSpotify) {
    tokio::spawn(async move {
        loop {
            spotify
                .refresh_token()
                .await
                .expect("Couldn't refresh user token!");
            tokio::time::sleep(tokio::time::Duration::from_secs(60 * 60)).await;
        }
    });
}

#[launch]
async fn rocket() -> _ {
    let (tx, rx) = watch::channel(String::from("init"));

    let creds = Credentials::from_env().unwrap();
    let oauth = OAuth::from_env(scopes!("user-read-currently-playing")).unwrap();
    let spotify = AuthCodeSpotify::new(creds.clone(), oauth.clone());

    dotenv().ok();
    let ref_token = env::var("RSPOTIFY_REFRESH_TOKEN").unwrap();
    let token = Token {
        refresh_token: Some(ref_token),
        ..Default::default()
    };

    *spotify.token.lock().await.unwrap() = Some(token);
    refresh_token(spotify.clone());

    send_to_endpoint(tx, spotify.clone());

    rocket::build()
        .manage(spotify)
        .manage(rx)
        .mount("/", routes![get_song])
}
