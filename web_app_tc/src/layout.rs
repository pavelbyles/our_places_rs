use topcoat::{
    Result,
    context::Cx,
    router::layout,
};

#[layout("/")]
pub async fn guest_layout(cx: &Cx, slot: Result) -> Result {
    web_app_common_tc::guest_base_layout(cx, slot).await
}
