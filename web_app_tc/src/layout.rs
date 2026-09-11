use topcoat::{
    Result,
    context::Cx,
    router::layout,
    view::{Child, View},
};

#[layout("/")]
pub async fn guest_layout(cx: &Cx, slot: Child<'_>) -> Result<impl View> {
    web_app_common_tc::guest_base_layout(cx, slot).await
}
