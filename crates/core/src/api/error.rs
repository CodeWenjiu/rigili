use gpui_kit::App;

use super::runtime::ApiTaskError;
use crate::login::expire;
use crate::text::api_error_message;

pub fn api_task_error_message(error: ApiTaskError, cx: &mut App) -> String {
    match error {
        ApiTaskError::Api(error) => {
            if error.is_login_required() {
                expire(cx);
            }
            api_error_message(&error)
        }
        ApiTaskError::Join(error) => format!("加载任务失败：{error}"),
    }
}
