pub fn doc_link(target: &str, doc: &str) -> String {
  format!(
    "https://triangle.haelp.dev/documents/{}.html#md:{}",
    doc, target
  )
}

pub fn troubleshooting_doc_link(target: &str) -> String {
  doc_link(target, "Troubleshooting")
}
