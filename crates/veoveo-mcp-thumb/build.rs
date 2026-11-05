//! Embeds the skills under `skills/`, which the server serves at `skill://thumb/…`.
fn main() {
    veoveo_skills::build::embed_skills("skills").expect("the server's skills");
}
