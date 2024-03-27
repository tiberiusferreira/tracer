pub fn create_db_overwriting_existing(docker_bin_path: &str, docker_compose_file_path: &str) {
    check_docker_exists(docker_bin_path);
    remove_existing_tracer_db(docker_bin_path, docker_compose_file_path);
    create_tracer_db(docker_bin_path, docker_compose_file_path);
}

fn check_docker_exists(docker_bin_path: &str) {
    println!("Checking if docker exists");
    let output = std::process::Command::new(docker_bin_path)
        .args(["-v"])
        .output()
        .expect("docker command did not run successfully");
    println!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success());
}
fn remove_existing_tracer_db(docker_bin_path: &str, docker_compose_file_path: &str) {
    println!("Removing existing tracer DB if any using docker compose -f tracer.yml down postgres");
    let output = std::process::Command::new(docker_bin_path)
        .args([
            "compose",
            "-f",
            docker_compose_file_path,
            "down",
            "postgres",
        ])
        .output()
        .expect("docker command did not run successfully");
    println!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success());
}

fn create_tracer_db(docker_bin_path: &str, docker_compose_file_path: &str) {
    println!("Creating a new tracer DB using docker compose -f tracer.yml up -d postgres");
    let output = std::process::Command::new(docker_bin_path)
        .args([
            "compose",
            "-f",
            docker_compose_file_path,
            "up",
            "-d",
            "postgres",
        ])
        .output()
        .expect("docker command did not run successfully");
    println!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success());
}

#[cfg(test)]
mod test {
    use crate::database_creation::{
        check_docker_exists, create_tracer_db, remove_existing_tracer_db,
    };

    #[test]
    fn can_create_and_destroy_db() {
        let docker_bin = "/usr/local/bin/docker";
        let compose_filepath = "./../tracer.yml";
        check_docker_exists(docker_bin);
        remove_existing_tracer_db(docker_bin, compose_filepath);
        create_tracer_db(docker_bin, compose_filepath);
        remove_existing_tracer_db(docker_bin, compose_filepath);
    }

    #[test]
    #[ignore]
    fn can_remove_tracer_db_if_exists() {
        remove_existing_tracer_db("/usr/local/bin/docker", "./../tracer.yml");
    }

    #[test]
    #[ignore]
    fn can_create_tracer_db() {
        create_tracer_db("/usr/local/bin/docker", "./../tracer.yml");
    }
}
