use ::url::Url;

pub fn to_safe_string(url: &Url) -> String {
    // We print only the hostnames to avoid leaking the API keys.
    url.host().map_or_else(|| "no host in url!".to_string(), |host| host.to_string())
}
