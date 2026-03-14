use std::io::Write;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SshError {
    #[error("Failed to connect: {0}")]
    ConnectionFailed(String),
    #[error("Authentication failed")]
    AuthFailed,
    #[error("Upload failed: {0}")]
    UploadFailed(String),
}

pub struct SshUploader {
    pub host: String,
}

impl SshUploader {
    pub fn new(host: String) -> Self {
        Self { host }
    }

    pub fn upload_file(
        &self,
        local_data: &[u8],
        remote_path: &str,
        _passphrase: &str,
    ) -> Result<String, SshError> {
        // Write data to a temp file so scp can read it
        let temp_path = {
            let mut path = std::env::temp_dir();
            path.push(format!("madera_upload_{}.tmp", std::process::id()));
            path
        };

        {
            let mut f = std::fs::File::create(&temp_path)
                .map_err(|e| SshError::UploadFailed(format!("Temp file creation failed: {}", e)))?;
            f.write_all(local_data)
                .map_err(|e| SshError::UploadFailed(format!("Temp file write failed: {}", e)))?;
        }

        // Build the scp destination: host is already in "user@ip" or "ip" format
        let destination = format!("{}:{}", self.host, remote_path);

        println!(
            "[SSH] Using system scp: {} -> {}",
            temp_path.display(),
            destination
        );

        // Run system scp (inherits the native OpenSSH agent automatically)
        let output = std::process::Command::new("scp")
            .arg("-o")
            .arg("StrictHostKeyChecking=no")
            .arg("-o")
            .arg("BatchMode=yes") // fail fast if agent can't auth
            .arg(temp_path.to_str().unwrap_or(""))
            .arg(&destination)
            .output()
            .map_err(|e| SshError::UploadFailed(format!("scp not found: {}", e)))?;

        // Clean up temp file
        let _ = std::fs::remove_file(&temp_path);

        if output.status.success() {
            println!("[SSH] Upload complete!");
            Ok(remote_path.to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let msg = if stderr.is_empty() { stdout } else { stderr };

            if msg.contains("Permission denied") || msg.contains("Authentication failed") {
                println!("[SSH] Auth failed: {}", msg);
                Err(SshError::AuthFailed)
            } else {
                println!("[SSH] Upload failed: {}", msg);
                Err(SshError::UploadFailed(msg.to_string()))
            }
        }
    }
}
