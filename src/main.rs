use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use askama::Template;
use axum::{
    extract::{Form, Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use rand_core::OsRng;
use serde::Deserialize;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use uuid::Uuid;
use tower_http::services::ServeDir;

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    sessions: Arc<Mutex<HashMap<String, i64>>>,
    admin_sessions: Arc<Mutex<HashMap<String, i64>>>,
}

#[derive(Deserialize)]
struct RegisterForm {
    student_id: String,
    full_name: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginForm {
    student_id: String,
    password: String,
}

#[derive(Deserialize)]
struct AdminLoginForm {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct CandidateForm {
    full_name: String,
    position: String,
    platform: String,
    photo_url: String,
}

#[derive(Deserialize)]
struct VoteForm {
    president: i64,
    vice_president: i64,
    secretary: i64,
    treasurer: i64,
}

#[derive(sqlx::FromRow)]
struct UserRecord {
    id: i64,
    full_name: String,
    password_hash: String,
    has_voted: bool,
}

#[derive(sqlx::FromRow)]
struct Candidate {
    id: i64,
    full_name: String,
    platform: String,
}

struct CandidateGroup {
    position: &'static str,
    field_name: &'static str,
    candidates: Vec<Candidate>,
}

#[derive(sqlx::FromRow)]
struct AdminRecord {
    id: i64,
    password_hash: String,
}

#[derive(sqlx::FromRow)]
struct AdminCandidate {
    id: i64,
    full_name: String,
    position: String,
    platform: String,
    photo_url: String,
}

#[derive(sqlx::FromRow)]
struct ResultRow {
    candidate_name: String,
    position: String,
    vote_count: i64,
}

#[derive(sqlx::FromRow)]
struct VoterSummary {
    student_id: String,
    full_name: String,
    has_voted: bool,
    created_at: String,
}

#[derive(Template)]
#[template(path = "register.html")]
struct RegisterTemplate<'a> {
    error: &'a str,
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate<'a> {
    error: &'a str,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate<'a> {
    full_name: &'a str,
}

#[derive(Template)]
#[template(path = "vote.html")]
struct VoteTemplate<'a> {
    groups: &'a [CandidateGroup],
    error: &'a str,
}

#[derive(Template)]
#[template(path = "confirmation.html")]
struct ConfirmationTemplate {
    president: i64,
    vice_president: i64,
    secretary: i64,
    treasurer: i64,
}

#[derive(Template)]
#[template(path = "success.html")]
struct SuccessTemplate;

#[derive(Template)]
#[template(path = "already_voted.html")]
struct AlreadyVotedTemplate;

#[derive(Template)]
#[template(path = "admin_login.html")]
struct AdminLoginTemplate<'a> {
    error: &'a str,
}

#[derive(Template)]
#[template(path = "admin_dashboard.html")]
struct AdminDashboardTemplate {
    total_voters: i64,
    total_votes: i64,
    voting_percentage: f64,
    voters: Vec<VoterSummary>,
}

#[derive(Template)]
#[template(path = "admin_candidates.html")]
struct AdminCandidatesTemplate<'a> {
    candidates: &'a [AdminCandidate],
    error: &'a str,
}

#[derive(Template)]
#[template(path = "results.html")]
struct ResultsTemplate<'a> {
    rows: &'a [ResultRow],
    total_votes: i64,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://rustvote.db".to_string());

    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    println!("Database connected successfully!");

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("Failed to run database migrations");

    println!("Database migrations completed successfully!");

    if let (Ok(username), Ok(password)) = (
        std::env::var("ADMIN_USERNAME"),
        std::env::var("ADMIN_PASSWORD"),
    ) {
        ensure_admin(&db, &username, &password)
            .await
            .expect("Failed to configure admin account");
    }

    let state = AppState {
        db,
        sessions: Arc::new(Mutex::new(HashMap::new())),
        admin_sessions: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .nest_service("/static", ServeDir::new("static"))
        .route("/", get(home))
        .route("/register", get(show_register).post(register))
        .route("/login", get(show_login).post(login))
        .route("/dashboard", get(dashboard))
        .route("/logout", post(logout))
        .route("/vote", get(show_vote).post(begin_vote))
        .route("/vote/confirmation", post(confirm_vote))
        .route("/vote/success", get(vote_success))
        .route("/results", get(results))
        .route("/admin/login", get(show_admin_login).post(admin_login))
        .route("/admin/logout", post(admin_logout))
        .route("/admin/dashboard", get(admin_dashboard))
        .route(
            "/admin/candidates",
            get(admin_candidates).post(add_candidate),
        )
        .route("/admin/candidates/{id}/edit", post(edit_candidate))
        .route("/admin/candidates/{id}/delete", post(delete_candidate))
        .with_state(state);

    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .expect("PORT must be a valid number");
    let address = format!("{host}:{port}")
        .parse::<SocketAddr>()
        .expect("HOST and PORT must form a valid address");
    println!("RustVote is running at http://{}", address);

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("Failed to bind to address");

    axum::serve(listener, app)
        .await
        .expect("Server failed");
}

async fn home() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>RustVote</title>
    <link rel="stylesheet" href="/static/css/style.css">
</head>
<body>
    <header class="site-header"><a class="brand" href="/">Rust<span class="brand-mark">Vote</span></a><nav class="site-nav"><a href="/admin/login">Admin</a></nav></header>
    <main class="page-narrow">
        <section class="panel">
            <p class="brand-mark">STUDENT COUNCIL ELECTION</p>
            <h1>Make your voice count.</h1>
            <p>Register, review the candidates, and cast one secure vote for each position.</p>
            <div class="actions"><a class="button" href="/register">Register to vote</a><a class="button button-secondary" href="/login">Log in</a></div>
        </section>
    </main>
    <footer>RustVote student council election</footer>
</body>
</html>"#,
    )
}

async fn show_register() -> Html<String> {
    render_register("")
}

async fn show_login() -> Html<String> {
    render_login("")
}

async fn register(State(state): State<AppState>, Form(form): Form<RegisterForm>) -> Response {
    let student_id = form.student_id.trim();
    let full_name = form.full_name.trim();

    if student_id.is_empty() || full_name.is_empty() || form.password.len() < 8 {
        return render_register(
            "Student ID and full name are required. Passwords must be at least 8 characters.",
        )
        .into_response();
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = match Argon2::default().hash_password(form.password.as_bytes(), &salt) {
        Ok(hash) => hash.to_string(),
        Err(_) => return render_register("Unable to create the account right now.").into_response(),
    };

    let result = sqlx::query(
        "INSERT INTO users (student_id, full_name, password_hash) VALUES (?, ?, ?)",
    )
    .bind(student_id)
    .bind(full_name)
    .bind(password_hash)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => RedirectResponse::new("/login").into_response(),
        Err(_) => render_register("That student ID is already registered.").into_response(),
    }
}

async fn login(State(state): State<AppState>, Form(form): Form<LoginForm>) -> Response {
    let user = sqlx::query_as::<_, UserRecord>(
        "SELECT id, full_name, password_hash, has_voted FROM users WHERE student_id = ?",
    )
    .bind(form.student_id.trim())
    .fetch_optional(&state.db)
    .await;

    let Ok(Some(user)) = user else {
        return render_login("Invalid student ID or password.").into_response();
    };

    let parsed_hash = match PasswordHash::new(&user.password_hash) {
        Ok(hash) => hash,
        Err(_) => return render_login("Invalid student ID or password.").into_response(),
    };

    if Argon2::default()
        .verify_password(form.password.as_bytes(), &parsed_hash)
        .is_err()
    {
        return render_login("Invalid student ID or password.").into_response();
    }

    let session_id = Uuid::new_v4().to_string();
    state
        .sessions
        .lock()
        .expect("session store lock poisoned")
        .insert(session_id.clone(), user.id);

    RedirectResponse::with_cookie("/dashboard", session_id).into_response()
}

async fn dashboard(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(user_id) = session_user_id(&state, &headers) else {
        return RedirectResponse::new("/login").into_response();
    };

    let Ok(Some(user)) = sqlx::query_as::<_, UserRecord>(
        "SELECT id, full_name, password_hash, has_voted FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    else {
        return RedirectResponse::new("/login").into_response();
    };

    Html(
        DashboardTemplate {
            full_name: &user.full_name,
        }
        .render()
        .expect("dashboard template failed to render"),
    )
    .into_response()
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(session_id) = cookie_value(&headers, "session_id") {
        state
            .sessions
            .lock()
            .expect("session store lock poisoned")
            .remove(&session_id);
    }

    RedirectResponse::clear_cookie("/login").into_response()
}

async fn show_vote(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(user_id) = session_user_id(&state, &headers) else {
        return RedirectResponse::new("/login").into_response();
    };

    let Ok(Some(user)) = sqlx::query_as::<_, UserRecord>(
        "SELECT id, full_name, password_hash, has_voted FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    else {
        return RedirectResponse::new("/login").into_response();
    };

    if user.has_voted {
        return Html(
            AlreadyVotedTemplate
                .render()
                .expect("already voted template failed to render"),
        )
        .into_response();
    }

    match load_candidate_groups(&state.db).await {
        Ok(groups) => render_vote(&groups, "").into_response(),
        Err(_) => render_vote(&[], "Unable to load candidates right now.").into_response(),
    }
}

async fn begin_vote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<VoteForm>,
) -> Response {
    let Some(user_id) = session_user_id(&state, &headers) else {
        return RedirectResponse::new("/login").into_response();
    };

    if !valid_vote_selection(&state.db, &form).await {
        return RedirectResponse::new("/vote").into_response();
    }

    let Ok(Some(user)) = sqlx::query_as::<_, UserRecord>(
        "SELECT id, full_name, password_hash, has_voted FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    else {
        return RedirectResponse::new("/login").into_response();
    };

    if user.has_voted {
        return Html(
            AlreadyVotedTemplate
                .render()
                .expect("already voted template failed to render"),
        )
        .into_response();
    }

    Html(
        ConfirmationTemplate {
            president: form.president,
            vice_president: form.vice_president,
            secretary: form.secretary,
            treasurer: form.treasurer,
        }
        .render()
        .expect("confirmation template failed to render"),
    )
    .into_response()
}

async fn confirm_vote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<VoteForm>,
) -> Response {
    let Some(user_id) = session_user_id(&state, &headers) else {
        return RedirectResponse::new("/login").into_response();
    };

    if !valid_vote_selection(&state.db, &form).await {
        return RedirectResponse::new("/vote").into_response();
    }

    let mut transaction = match state.db.begin().await {
        Ok(transaction) => transaction,
        Err(_) => return RedirectResponse::new("/vote").into_response(),
    };

    let claimed_vote = sqlx::query(
        "UPDATE users SET has_voted = TRUE WHERE id = ? AND has_voted = FALSE",
    )
    .bind(user_id)
    .execute(&mut *transaction)
    .await;

    if !matches!(claimed_vote, Ok(result) if result.rows_affected() == 1) {
        return Html(
            AlreadyVotedTemplate
                .render()
                .expect("already voted template failed to render"),
        )
        .into_response();
    }

    let selections = [
        (form.president, "President"),
        (form.vice_president, "Vice President"),
        (form.secretary, "Secretary"),
        (form.treasurer, "Treasurer"),
    ];

    for (candidate_id, position) in selections {
        let result = sqlx::query(
            "INSERT INTO votes (user_id, candidate_id, position) VALUES (?, ?, ?)",
        )
        .bind(user_id)
        .bind(candidate_id)
        .bind(position)
        .execute(&mut *transaction)
        .await;

        if result.is_err() {
            return RedirectResponse::new("/vote").into_response();
        }
    }

    if transaction.commit().await.is_err() {
        return RedirectResponse::new("/vote").into_response();
    }

    RedirectResponse::new("/vote/success").into_response()
}

async fn vote_success(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if session_user_id(&state, &headers).is_none() {
        return RedirectResponse::new("/login").into_response();
    }

    Html(
        SuccessTemplate
            .render()
            .expect("success template failed to render"),
    )
    .into_response()
}

async fn ensure_admin(
    db: &SqlitePool,
    username: &str,
    password: &str,
) -> Result<(), sqlx::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("failed to hash admin password")
        .to_string();

    sqlx::query(
        "INSERT INTO admins (username, password_hash) VALUES (?, ?)
         ON CONFLICT(username) DO UPDATE SET password_hash = excluded.password_hash",
    )
    .bind(username)
    .bind(password_hash)
    .execute(db)
    .await?;

    Ok(())
}

async fn show_admin_login() -> Html<String> {
    render_admin_login("")
}

async fn admin_login(
    State(state): State<AppState>,
    Form(form): Form<AdminLoginForm>,
) -> Response {
    let admin = sqlx::query_as::<_, AdminRecord>(
        "SELECT id, password_hash FROM admins WHERE username = ?",
    )
    .bind(form.username.trim())
    .fetch_optional(&state.db)
    .await;

    let Ok(Some(admin)) = admin else {
        return render_admin_login("Invalid admin username or password.").into_response();
    };

    let Ok(parsed_hash) = PasswordHash::new(&admin.password_hash) else {
        return render_admin_login("Invalid admin username or password.").into_response();
    };

    if Argon2::default()
        .verify_password(form.password.as_bytes(), &parsed_hash)
        .is_err()
    {
        return render_admin_login("Invalid admin username or password.").into_response();
    }

    let session_id = Uuid::new_v4().to_string();
    state
        .admin_sessions
        .lock()
        .expect("admin session store lock poisoned")
        .insert(session_id.clone(), admin.id);

    RedirectResponse::with_named_cookie("/admin/dashboard", "admin_session_id", session_id)
        .into_response()
}

async fn admin_logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(session_id) = cookie_value(&headers, "admin_session_id") {
        state
            .admin_sessions
            .lock()
            .expect("admin session store lock poisoned")
            .remove(&session_id);
    }

    RedirectResponse::clear_named_cookie("/admin/login", "admin_session_id").into_response()
}

async fn admin_dashboard(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if admin_session_id(&state, &headers).is_none() {
        return RedirectResponse::new("/admin/login").into_response();
    }

    let total_voters = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await;
    let total_votes = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM votes")
        .fetch_one(&state.db)
        .await;
    let voted_voters = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM users WHERE has_voted = TRUE",
    )
    .fetch_one(&state.db)
    .await;

    let (Ok((total_voters,)), Ok((total_votes,)), Ok((voted_voters,))) =
        (total_voters, total_votes, voted_voters)
    else {
        return Html("<h1>Unable to load admin statistics.</h1>").into_response();
    };

    let voting_percentage = if total_voters == 0 {
        0.0
    } else {
        (voted_voters as f64 / total_voters as f64) * 100.0
    };

    let voters = sqlx::query_as::<_, VoterSummary>(
        "SELECT student_id, full_name, has_voted, created_at
         FROM users ORDER BY created_at DESC, full_name ASC",
    )
    .fetch_all(&state.db)
    .await;

    let Ok(voters) = voters else {
        return Html("<h1>Unable to load voter records.</h1>").into_response();
    };

    Html(
        AdminDashboardTemplate {
            total_voters,
            total_votes,
            voting_percentage,
            voters,
        }
        .render()
        .expect("admin dashboard template failed to render"),
    )
    .into_response()
}

async fn results(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if admin_session_id(&state, &headers).is_none() {
        return RedirectResponse::new("/admin/login").into_response();
    }

    let rows = sqlx::query_as::<_, ResultRow>(
        "SELECT candidates.full_name AS candidate_name,
                candidates.position,
                COUNT(votes.id) AS vote_count
         FROM candidates
         LEFT JOIN votes ON votes.candidate_id = candidates.id
         GROUP BY candidates.id
         ORDER BY candidates.position, vote_count DESC, candidates.full_name",
    )
    .fetch_all(&state.db)
    .await;
    let total_votes = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM votes")
        .fetch_one(&state.db)
        .await;

    let (Ok(rows), Ok((total_votes,))) = (rows, total_votes) else {
        return Html("<h1>Unable to load election results.</h1>").into_response();
    };

    Html(
        ResultsTemplate {
            rows: &rows,
            total_votes,
        }
        .render()
        .expect("results template failed to render"),
    )
    .into_response()
}

async fn admin_candidates(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if admin_session_id(&state, &headers).is_none() {
        return RedirectResponse::new("/admin/login").into_response();
    }

    match load_admin_candidates(&state.db).await {
        Ok(candidates) => render_admin_candidates(&candidates, "").into_response(),
        Err(_) => render_admin_candidates(&[], "Unable to load candidates.").into_response(),
    }
}

async fn add_candidate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<CandidateForm>,
) -> Response {
    if admin_session_id(&state, &headers).is_none() {
        return RedirectResponse::new("/admin/login").into_response();
    }

    if form.full_name.trim().is_empty() || form.position.trim().is_empty() {
        return render_admin_candidates(&[], "Name and position are required.").into_response();
    }

    let photo_url = (!form.photo_url.trim().is_empty()).then(|| form.photo_url.trim());
    let result = sqlx::query(
        "INSERT INTO candidates (full_name, position, platform, photo_url)
         VALUES (?, ?, ?, ?)",
    )
    .bind(form.full_name.trim())
    .bind(form.position.trim())
    .bind(form.platform.trim())
    .bind(photo_url)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => RedirectResponse::new("/admin/candidates").into_response(),
        Err(_) => render_admin_candidates(&[], "Unable to add candidate.").into_response(),
    }
}

async fn edit_candidate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Form(form): Form<CandidateForm>,
) -> Response {
    if admin_session_id(&state, &headers).is_none() {
        return RedirectResponse::new("/admin/login").into_response();
    }

    let photo_url = (!form.photo_url.trim().is_empty()).then(|| form.photo_url.trim());
    let result = sqlx::query(
        "UPDATE candidates SET full_name = ?, position = ?, platform = ?, photo_url = ?
         WHERE id = ?",
    )
    .bind(form.full_name.trim())
    .bind(form.position.trim())
    .bind(form.platform.trim())
    .bind(photo_url)
    .bind(id)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => RedirectResponse::new("/admin/candidates").into_response(),
        Err(_) => RedirectResponse::new("/admin/candidates").into_response(),
    }
}

async fn delete_candidate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Response {
    if admin_session_id(&state, &headers).is_none() {
        return RedirectResponse::new("/admin/login").into_response();
    }

    let result = sqlx::query("DELETE FROM candidates WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => RedirectResponse::new("/admin/candidates").into_response(),
        Err(_) => RedirectResponse::new("/admin/candidates").into_response(),
    }
}

async fn load_admin_candidates(db: &SqlitePool) -> Result<Vec<AdminCandidate>, sqlx::Error> {
    sqlx::query_as::<_, AdminCandidate>(
        "SELECT id, full_name, position, COALESCE(platform, '') AS platform,
                COALESCE(photo_url, '') AS photo_url
         FROM candidates ORDER BY position, full_name",
    )
    .fetch_all(db)
    .await
}

async fn load_candidate_groups(db: &SqlitePool) -> Result<Vec<CandidateGroup>, sqlx::Error> {
    let positions = [
        ("President", "president"),
        ("Vice President", "vice_president"),
        ("Secretary", "secretary"),
        ("Treasurer", "treasurer"),
    ];
    let mut groups = Vec::new();

    for (position, field_name) in positions {
        let candidates = sqlx::query_as::<_, Candidate>(
            "SELECT id, full_name, COALESCE(platform, '') AS platform
             FROM candidates WHERE position = ? ORDER BY full_name",
        )
        .bind(position)
        .fetch_all(db)
        .await?;

        groups.push(CandidateGroup {
            position,
            field_name,
            candidates,
        });
    }

    Ok(groups)
}

async fn valid_vote_selection(db: &SqlitePool, form: &VoteForm) -> bool {
    let selections = [
        (form.president, "President"),
        (form.vice_president, "Vice President"),
        (form.secretary, "Secretary"),
        (form.treasurer, "Treasurer"),
    ];

    for (candidate_id, expected_position) in selections {
        let candidate = sqlx::query_as::<_, (String,)>(
            "SELECT position FROM candidates WHERE id = ?",
        )
        .bind(candidate_id)
        .fetch_optional(db)
        .await;

        if !matches!(candidate, Ok(Some((position,))) if position == expected_position) {
            return false;
        }
    }

    true
}

fn session_user_id(state: &AppState, headers: &HeaderMap) -> Option<i64> {
    let session_id = cookie_value(headers, "session_id")?;
    state
        .sessions
        .lock()
        .expect("session store lock poisoned")
        .get(&session_id)
        .copied()
}

    fn admin_session_id(state: &AppState, headers: &HeaderMap) -> Option<i64> {
        let session_id = cookie_value(headers, "admin_session_id")?;
        state
        .admin_sessions
        .lock()
        .expect("admin session store lock poisoned")
        .get(&session_id)
        .copied()
    }

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_header.split(';').find_map(|cookie| {
        let (key, value) = cookie.trim().split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}

fn render_register(error: &str) -> Html<String> {
    Html(
        RegisterTemplate { error }
            .render()
            .expect("register template failed to render"),
    )
}

fn render_login(error: &str) -> Html<String> {
    Html(
        LoginTemplate { error }
            .render()
            .expect("login template failed to render"),
    )
}

fn render_vote<'a>(groups: &'a [CandidateGroup], error: &'a str) -> Html<String> {
    Html(
        VoteTemplate { groups, error }
            .render()
            .expect("vote template failed to render"),
    )
}

fn render_admin_login(error: &str) -> Html<String> {
    Html(
        AdminLoginTemplate { error }
            .render()
            .expect("admin login template failed to render"),
    )
}

fn render_admin_candidates(candidates: &[AdminCandidate], error: &str) -> Html<String> {
    Html(
        AdminCandidatesTemplate { candidates, error }
            .render()
            .expect("admin candidates template failed to render"),
    )
}

struct RedirectResponse;

impl RedirectResponse {
    fn new(location: &str) -> Response {
        (StatusCode::SEE_OTHER, [(header::LOCATION, location)]).into_response()
    }

    fn with_cookie(location: &str, session_id: String) -> Response {
        Self::with_named_cookie(location, "session_id", session_id)
    }

    fn with_named_cookie(location: &str, name: &str, session_id: String) -> Response {
        let cookie = format!("{name}={session_id}; HttpOnly; SameSite=Lax; Path=/");
        (
            StatusCode::SEE_OTHER,
            [
                (header::LOCATION, location),
                (header::SET_COOKIE, cookie.as_str()),
            ],
        )
            .into_response()
    }

    fn clear_cookie(location: &str) -> Response {
        Self::clear_named_cookie(location, "session_id")
    }

    fn clear_named_cookie(location: &str, name: &str) -> Response {
        let cookie = format!("{name}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0");
        (
            StatusCode::SEE_OTHER,
            [
                (header::LOCATION, location),
                (header::SET_COOKIE, cookie.as_str()),
            ],
        )
            .into_response()
    }
}