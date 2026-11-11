use gpui_kit::App;

use rigili_core::runtime::ApiTaskError;

pub(crate) fn api_task_error_message(error: ApiTaskError, cx: &mut App) -> String {
    match error {
        ApiTaskError::Api(error) => {
            if error.is_login_required() {
                crate::login::expire(cx);
            }
            rigili_core::api_error_message(&error)
        }
        ApiTaskError::Join(error) => format!("加载任务失败：{error}"),
    }
}
