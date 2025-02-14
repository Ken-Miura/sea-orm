use actix3_example_core::{
    sea_orm::{Database, DatabaseConnection},
    Mutation, Query,
};
use actix_files as fs;
use actix_web::{
    error, get, middleware, post, web, App, Error, HttpRequest, HttpResponse, HttpServer, Result,
};

use entity::post;
use listenfd::ListenFd;
use migration::{Migrator, MigratorTrait};
use serde::{Deserialize, Serialize};
use std::env;
use tera::Tera;

const DEFAULT_POSTS_PER_PAGE: u64 = 5;

#[derive(Debug, Clone)]
struct AppState {
    templates: tera::Tera,
    conn: DatabaseConnection,
}
#[derive(Debug, Deserialize)]
pub struct Params {
    page: Option<u64>,
    posts_per_page: Option<u64>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct FlashData {
    kind: String,
    message: String,
}

#[get("/")]
async fn list(
    req: HttpRequest,
    data: web::Data<AppState>,
    opt_flash: Option<actix_flash::Message<FlashData>>,
) -> Result<HttpResponse, Error> {
    let template = &data.templates;
    let conn = &data.conn;

    // get params
    let params = web::Query::<Params>::from_query(req.query_string()).unwrap();

    let page = params.page.unwrap_or(1);
    let posts_per_page = params.posts_per_page.unwrap_or(DEFAULT_POSTS_PER_PAGE);

    let (posts, num_pages) = Query::find_posts_in_page(conn, page, posts_per_page)
        .await
        .expect("Cannot find posts in page");

    let mut ctx = tera::Context::new();
    ctx.insert("posts", &posts);
    ctx.insert("page", &page);
    ctx.insert("posts_per_page", &posts_per_page);
    ctx.insert("num_pages", &num_pages);

    if let Some(flash) = opt_flash {
        let flash_inner = flash.into_inner();
        ctx.insert("flash", &flash_inner);
    }

    let body = template
        .render("index.html.tera", &ctx)
        .map_err(|_| error::ErrorInternalServerError("Template error"))?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

#[get("/new")]
async fn new(data: web::Data<AppState>) -> Result<HttpResponse, Error> {
    let template = &data.templates;
    let ctx = tera::Context::new();
    let body = template
        .render("new.html.tera", &ctx)
        .map_err(|_| error::ErrorInternalServerError("Template error"))?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

#[post("/")]
async fn create(
    data: web::Data<AppState>,
    post_form: web::Form<post::Model>,
) -> actix_flash::Response<HttpResponse, FlashData> {
    let conn = &data.conn;

    let form = post_form.into_inner();

    Mutation::create_post(conn, form)
        .await
        .expect("could not insert post");

    let flash = FlashData {
        kind: "success".to_owned(),
        message: "Post successfully added.".to_owned(),
    };

    actix_flash::Response::with_redirect(flash, "/")
}

#[get("/{id}")]
async fn edit(data: web::Data<AppState>, id: web::Path<i32>) -> Result<HttpResponse, Error> {
    let conn = &data.conn;
    let template = &data.templates;
    let id = id.into_inner();

    let post: post::Model = Query::find_post_by_id(conn, id)
        .await
        .expect("could not find post")
        .unwrap_or_else(|| panic!("could not find post with id {}", id));

    let mut ctx = tera::Context::new();
    ctx.insert("post", &post);

    let body = template
        .render("edit.html.tera", &ctx)
        .map_err(|_| error::ErrorInternalServerError("Template error"))?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

#[post("/{id}")]
async fn update(
    data: web::Data<AppState>,
    id: web::Path<i32>,
    post_form: web::Form<post::Model>,
) -> actix_flash::Response<HttpResponse, FlashData> {
    let conn = &data.conn;
    let form = post_form.into_inner();
    let id = id.into_inner();

    Mutation::update_post_by_id(conn, id, form)
        .await
        .expect("could not edit post");

    let flash = FlashData {
        kind: "success".to_owned(),
        message: "Post successfully updated.".to_owned(),
    };

    actix_flash::Response::with_redirect(flash, "/")
}

#[post("/delete/{id}")]
async fn delete(
    data: web::Data<AppState>,
    id: web::Path<i32>,
) -> actix_flash::Response<HttpResponse, FlashData> {
    let conn = &data.conn;
    let id = id.into_inner();

    Mutation::delete_post(conn, id)
        .await
        .expect("could not delete post");

    let flash = FlashData {
        kind: "success".to_owned(),
        message: "Post successfully deleted.".to_owned(),
    };

    actix_flash::Response::with_redirect(flash, "/")
}

#[actix_web::main]
async fn start() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "debug");
    tracing_subscriber::fmt::init();

    // get env vars
    dotenv::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL is not set in .env file");
    let host = env::var("HOST").expect("HOST is not set in .env file");
    let port = env::var("PORT").expect("PORT is not set in .env file");
    let server_url = format!("{}:{}", host, port);

    // create post table if not exists
    let conn = Database::connect(&db_url).await.unwrap();
    Migrator::up(&conn, None).await.unwrap();
    let templates = Tera::new(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*")).unwrap();
    let state = AppState { templates, conn };

    let mut listenfd = ListenFd::from_env();
    let mut server = HttpServer::new(move || {
        App::new()
            .data(state.clone())
            .wrap(middleware::Logger::default()) // enable logger
            .wrap(actix_flash::Flash::default())
            .configure(init)
            .service(fs::Files::new("/static", "./api/static").show_files_listing())
    });

    server = match listenfd.take_tcp_listener(0)? {
        Some(listener) => server.listen(listener)?,
        None => server.bind(&server_url)?,
    };

    println!("Starting server at {}", server_url);
    server.run().await?;

    Ok(())
}

fn init(cfg: &mut web::ServiceConfig) {
    cfg.service(list);
    cfg.service(new);
    cfg.service(create);
    cfg.service(edit);
    cfg.service(update);
    cfg.service(delete);
}

pub fn main() {
    let result = start();

    if let Some(err) = result.err() {
        println!("Error: {}", err)
    }
}
