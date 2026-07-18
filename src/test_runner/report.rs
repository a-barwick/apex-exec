use super::{TestFailure, TestReport};

impl TestReport {
    pub fn render_console(&self) -> String {
        let mut lines = Vec::new();
        for test in &self.tests {
            if let Some(failure) = &test.failure {
                lines.push(format!("FAIL {}: {}", test.name, failure.summary()));
                for line in failure.rendered.lines() {
                    lines.push(format!("  {line}"));
                }
            } else {
                lines.push(format!("PASS {}", test.name));
            }
            for output in &test.output {
                lines.push(format!("  debug: {output}"));
            }
        }
        lines.push(String::new());
        lines.push("Coverage:".to_owned());
        if self.coverage.files.is_empty() {
            lines.push("  no executable production lines".to_owned());
        } else {
            for file in &self.coverage.files {
                lines.push(format!(
                    "  {}: {}/{} lines ({:.2}%), {}/{} branches ({:.2}%)",
                    file.path.display(),
                    file.covered_lines,
                    file.total_lines,
                    percentage(file.covered_lines, file.total_lines),
                    file.covered_branches,
                    file.total_branches,
                    percentage(file.covered_branches, file.total_branches),
                ));
            }
        }
        lines.push(format!(
            "Summary: {} passed, {} failed, {} total; {}/{} lines ({:.2}%), {}/{} branches ({:.2}%)",
            self.passed(),
            self.failed(),
            self.tests.len(),
            self.coverage.covered_lines,
            self.coverage.total_lines,
            percentage(self.coverage.covered_lines, self.coverage.total_lines),
            self.coverage.covered_branches,
            self.coverage.total_branches,
            percentage(
                self.coverage.covered_branches,
                self.coverage.total_branches
            ),
        ));
        lines.join("\n")
    }

    pub fn to_junit_xml(&self) -> String {
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"apex-exec\" tests=\"{}\" failures=\"{}\" time=\"0\">\n",
            self.tests.len(),
            self.failed()
        );
        xml.push_str("  <properties>\n");
        xml.push_str(&format!(
            "    <property name=\"line-rate\" value=\"{:.6}\"/>\n",
            rate(self.coverage.covered_lines, self.coverage.total_lines)
        ));
        xml.push_str(&format!(
            "    <property name=\"branch-rate\" value=\"{:.6}\"/>\n",
            rate(self.coverage.covered_branches, self.coverage.total_branches)
        ));
        xml.push_str("  </properties>\n");
        for test in &self.tests {
            xml.push_str(&format!(
                "  <testcase classname=\"{}\" name=\"{}\" time=\"0\">\n",
                xml_escape(&test.class_name),
                xml_escape(&test.method_name)
            ));
            if let Some(failure) = &test.failure {
                xml.push_str(&format!(
                    "    <failure type=\"{}\" message=\"{}\">{}</failure>\n",
                    xml_escape(failure.exception_type.as_deref().unwrap_or("RuntimeError")),
                    xml_escape(&failure.message),
                    xml_escape(&failure.rendered)
                ));
            }
            if !test.output.is_empty() {
                xml.push_str(&format!(
                    "    <system-out>{}</system-out>\n",
                    xml_escape(&test.output.join("\n"))
                ));
            }
            xml.push_str("  </testcase>\n");
        }
        xml.push_str("</testsuite>\n");
        xml
    }

    /// Renders deterministic Cobertura 0.4 XML for CI coverage consumers.
    pub fn to_cobertura_xml(&self) -> String {
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<coverage line-rate=\"{:.6}\" branch-rate=\"{:.6}\" lines-covered=\"{}\" lines-valid=\"{}\" branches-covered=\"{}\" branches-valid=\"{}\" version=\"apex-exec\" timestamp=\"0\">\n",
            rate(self.coverage.covered_lines, self.coverage.total_lines),
            rate(self.coverage.covered_branches, self.coverage.total_branches),
            self.coverage.covered_lines,
            self.coverage.total_lines,
            self.coverage.covered_branches,
            self.coverage.total_branches,
        );
        xml.push_str("  <sources><source>.</source></sources>\n  <packages>\n");
        for file in &self.coverage.files {
            xml.push_str(&format!(
                "    <package name=\"{}\" line-rate=\"{:.6}\" branch-rate=\"{:.6}\">\n      <classes>\n        <class name=\"{}\" filename=\"{}\" line-rate=\"{:.6}\" branch-rate=\"{:.6}\">\n          <methods/>\n          <lines>\n",
                xml_escape(&file.path.display().to_string()),
                rate(file.covered_lines, file.total_lines),
                rate(file.covered_branches, file.total_branches),
                xml_escape(&file.path.display().to_string()),
                xml_escape(&file.path.display().to_string()),
                rate(file.covered_lines, file.total_lines),
                rate(file.covered_branches, file.total_branches),
            ));
            for line in &file.executable_line_numbers {
                let hits = usize::from(file.covered_line_numbers.binary_search(line).is_ok());
                xml.push_str(&format!(
                    "            <line number=\"{line}\" hits=\"{hits}\"/>\n"
                ));
            }
            xml.push_str(
                "          </lines>\n        </class>\n      </classes>\n    </package>\n",
            );
        }
        xml.push_str("  </packages>\n</coverage>\n");
        xml
    }
}

impl TestFailure {
    fn summary(&self) -> String {
        self.exception_type.as_ref().map_or_else(
            || self.message.clone(),
            |ty| format!("{ty}: {}", self.message),
        )
    }
}

fn percentage(covered: usize, total: usize) -> f64 {
    rate(covered, total) * 100.0
}

fn rate(covered: usize, total: usize) -> f64 {
    if total == 0 {
        1.0
    } else {
        covered as f64 / total as f64
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
