use std::io::Write;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SshError {
    #[error("Failed to connect: {0}")]
    ConnectionFailed(String),
    #[error("Authentication failed: {0}")]
    AuthFailed(String),
    #[error("Remote path is not writable or does not exist: {0}")]
    RemotePathFailed(String),
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

    pub fn test_remote_path(&self, remote_path: &str, _passphrase: &str) -> Result<(), SshError> {
        let remote_path = remote_path.trim_end_matches('/');
        let quoted_path = shell_quote(remote_path);
        let check_command = format!("test -d {0} && test -w {0}", quoted_path);

        println!("[SSH] Testing remote path: {}:{}", self.host, remote_path);

        let output = std::process::Command::new(ssh_cmd())
            .arg("-o")
            .arg("StrictHostKeyChecking=no")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg(&self.host)
            .arg(check_command)
            .output()
            .map_err(|e| SshError::UploadFailed(format!("ssh not found: {}", e)))?;

        if output.status.success() {
            println!("[SSH] Remote path test complete!");
            Ok(())
        } else {
            let msg = command_output_message(&output);
            println!("[SSH] Remote path test failed: {}", msg);
            Err(self.classify_failure(&msg, remote_path))
        }
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
        // Use Windows native OpenSSH scp so it can talk to the Windows SSH agent
        let output = std::process::Command::new(scp_cmd())
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
            let msg = command_output_message(&output);
            println!("[SSH] Upload failed: {}", msg);
            Err(self.classify_failure(&msg, remote_path))
        }
    }

    fn classify_failure(&self, msg: &str, remote_path: &str) -> SshError {
        if msg.contains("Permission denied") || msg.contains("Authentication failed") {
            SshError::AuthFailed(format!(
                "OpenSSH could not authenticate to {}. Check the username, SSH key, and ssh-agent. Details: {}",
                self.host, msg
            ))
        } else if msg.contains("Name or service not known")
            || msg.contains("Temporary failure")
            || msg.contains("Could not resolve hostname")
            || msg.contains("Connection timed out")
            || msg.contains("Connection refused")
            || msg.contains("No route to host")
            || msg.contains("Network is unreachable")
            || msg.contains("Operation timed out")
        {
            SshError::ConnectionFailed(format!(
                "Cannot connect to {}. Check the server address and port 22. Details: {}",
                self.host, msg
            ))
        } else if msg.contains("No such file or directory")
            || msg.contains("not a directory")
            || msg.contains("Failure")
        {
            SshError::RemotePathFailed(format!(
                "The remote destination '{}' could not be written. Create the folder and verify permissions. Details: {}",
                remote_path, msg
            ))
        } else {
            SshError::UploadFailed(msg.to_string())
        }
    }
}

fn ssh_cmd() -> &'static str {
    if cfg!(target_os = "windows") {
        r"C:\Windows\System32\OpenSSH\ssh.exe"
    } else {
        "ssh"
    }
}

fn scp_cmd() -> &'static str {
    if cfg!(target_os = "windows") {
        r"C:\Windows\System32\OpenSSH\scp.exe"
    } else {
        "scp"
    }
}

fn command_output_message(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let output_msg = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };

    if output_msg.is_empty() {
        format!("command exited with status {}", output.status)
    } else {
        output_msg.to_string()
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
