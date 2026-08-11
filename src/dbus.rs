use std::collections::HashMap;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, Value};

pub struct DBus {
    connection: Connection,
}

impl DBus {
    pub fn new() -> zbus::Result<DBus> {
        let connection = Connection::session()?;
        Ok(DBus { connection })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

/// Captures a screenshot using D-Bus interface (org.freedesktop.portal.Screenshot) and returns a URI. Requires a D-Bus connection.
pub fn get_screenshot(conn: &Connection) -> zbus::Result<String> {
    // Create a proxy to the screenshot portal
    let screenshot_proxy = Proxy::new(
        conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Screenshot",
    )?;

    let mut options: HashMap<&str, Value> = HashMap::new();
    options.insert("interactive", Value::from(false));

    let request_path: OwnedObjectPath = screenshot_proxy.call(
        "Screenshot",
        &("", options),
    )?;

    // Create a proxy to listen to the specific Request object returned above
    let request_proxy = Proxy::new(
        conn,
        "org.freedesktop.portal.Desktop",
        &request_path,
        "org.freedesktop.portal.Request"
    )?;

    // Wait for the 'Response' signal (this blocks the thread until KDE finishes the capture)
    let mut signal_iterator = request_proxy.receive_signal("Response")?;
    let message = signal_iterator.next().unwrap();

    // Parse the Response signal body
    // Signature is (u32, a{sv}): response code, and a dictionary of results
    let binding = message.body();
    let (response_code, results): (u32, HashMap<String, Value>) = binding.deserialize()?;

    // Response code 0 means success. 1 is user canceled, 2 is other error.
    if response_code != 0 {
        return Err(zbus::Error::Failure(format!("Screenshot portal returned code {}", response_code).into()));
    }

    // Extract the URI from the results dictionary
    if let Some(Value::Str(uri)) = results.get("uri") {
        Ok(uri.as_str().to_string())
    } else {
        Err(zbus::Error::Failure("Portal response did not contain a URI".into()))
    }
}