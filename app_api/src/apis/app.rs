use actix_web::{web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::str;

#[derive(Deserialize, Serialize, Debug)]
struct HelloResponse {
    response: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct BannerImgUrlResponse {
    url: String,
    title: String,
    description: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct BannerImgUrlResponses {
    responses: Vec<BannerImgUrlResponse>,
}

pub async fn greet_no_name(_req: HttpRequest) -> HttpResponse {
    let resp = HelloResponse {
        response: "Hello World!".to_string(),
    };

    log::debug!("Received request for: greet_no_name");

    HttpResponse::Ok().json(resp)
}

pub async fn greet_with_name(_req: HttpRequest, name: web::Path<String>) -> HttpResponse {
    let resp = HelloResponse {
        response: format!("Hello {}!", name),
    };

    log::debug!("Received request for: greet_with_name/{:?}", name);

    HttpResponse::Ok().json(resp)
}

// TODO: Change this to pull URL and titles from a properties file
pub async fn get_banner_image_urls(_req: HttpRequest) -> HttpResponse {
    let resp = BannerImgUrlResponses {
        responses: vec![
            BannerImgUrlResponse {
                url: "https://www.myimage.com/image1.png".to_string(),
                title: "First image".to_string(),
                description: "Image of condo1".to_string(),
            },
            BannerImgUrlResponse {
                url: "https://www.myimage.com/image2.png".to_string(),
                title: "Second image".to_string(),
                description: "Image of condo2".to_string(),
            },
            BannerImgUrlResponse {
                url: "https://www.myimage.com/image3.png".to_string(),
                title: "Third image".to_string(),
                description: "Image of condo3".to_string(),
            },
        ],
    };

    log::debug!("Received request for: get_banner_image_urls");

    HttpResponse::Ok().json(resp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{body::to_bytes, http::StatusCode, test};

    #[actix_web::test]
    async fn test_greet_no_name_ok() {
        let req = test::TestRequest::default().to_http_request();
        let http_resp = greet_no_name(req).await;

        assert_eq!(http_resp.status(), StatusCode::OK);
        let body_bytes = to_bytes(http_resp.into_body()).await.unwrap();
        let hello_resp: HelloResponse =
            serde_json::from_str(str::from_utf8(&body_bytes).unwrap()).unwrap();
        assert_eq!("Hello World!".to_string(), hello_resp.response)
    }

    #[actix_web::test]
    async fn test_greet_with_name_ok() {
        let test_names = vec!["pavel", "kristina", "laila", "ethan"];

        for test_name in test_names {
            let req = test::TestRequest::default().to_http_request();
            let http_resp = greet_with_name(req, web::Path::from(test_name.to_string())).await;

            assert_eq!(http_resp.status(), StatusCode::OK);
            let body_bytes_result = to_bytes(http_resp.into_body()).await;
            let body_bytes = match body_bytes_result {
                Ok(bytes) => bytes,
                Err(err) => panic!("Error occurred: {}", err),
            };
            let str_result = str::from_utf8(&body_bytes);
            let str = match str_result {
                Ok(str) => str,
                Err(err) => panic!("Error occurred: {}", err),
            };
            let hello_resp: HelloResponse = match serde_json::from_str(str) {
                Ok(json_obj) => json_obj,
                Err(err) => panic!("Error occurred: {}", err),
            };
            assert_eq!(format!("Hello {}!", test_name), hello_resp.response);
        }
    }
}
