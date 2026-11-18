use serde::Deserialize;

rigili_macro::mod_flat!(
    account, comment, danmaku, live, message, mine, pgc, search, space, video
);

pub(crate) fn null_or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}
