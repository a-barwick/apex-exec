use crate::{ast::ClassMember, hir::ClassMemberId, project::Compilation};

#[derive(Clone, Debug)]
pub(super) struct TestCase {
    pub(super) name: String,
    pub(super) class_name: String,
    pub(super) method_name: String,
    pub(super) target: ClassMemberId,
    pub(super) setup_methods: Vec<ClassMemberId>,
}

pub(super) fn discover_tests(compilation: &Compilation, filter: Option<&str>) -> Vec<TestCase> {
    let mut cases = Vec::new();
    for (class_id, class) in compilation.program.classes.iter().enumerate() {
        if !class
            .annotations
            .iter()
            .any(|annotation| annotation.kind.is_test())
        {
            continue;
        }
        let setup_methods = class
            .members
            .iter()
            .enumerate()
            .filter_map(|(member_id, member)| match member {
                ClassMember::Method(method)
                    if method
                        .annotations
                        .iter()
                        .any(|annotation| annotation.kind.is_test_setup()) =>
                {
                    Some(ClassMemberId {
                        class_id,
                        member_id,
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for (member_id, member) in class.members.iter().enumerate() {
            let ClassMember::Method(method) = member else {
                continue;
            };
            if !method
                .annotations
                .iter()
                .any(|annotation| annotation.kind.is_test())
            {
                continue;
            }
            let name = format!("{}.{}", class.name.spelling, method.name.spelling);
            if !matches_filter(filter, &class.name.spelling, &method.name.spelling, &name) {
                continue;
            }
            cases.push(TestCase {
                name,
                class_name: class.name.spelling.clone(),
                method_name: method.name.spelling.clone(),
                target: ClassMemberId {
                    class_id,
                    member_id,
                },
                setup_methods: setup_methods.clone(),
            });
        }
    }
    cases.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    cases
}

pub(super) fn matches_filter(
    filter: Option<&str>,
    class: &str,
    method: &str,
    full_name: &str,
) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    let filter = filter.to_ascii_lowercase();
    let class = class.to_ascii_lowercase();
    let method = method.to_ascii_lowercase();
    let full_name = full_name.to_ascii_lowercase();
    if filter.contains('*') {
        wildcard_matches(&filter, &full_name)
    } else if filter.contains('.') {
        filter == full_name
    } else {
        filter == class || filter == method
    }
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let parts = pattern.split('*').collect::<Vec<_>>();
    let mut cursor = 0usize;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(found) = value[cursor..].find(part) else {
            return false;
        };
        if index == 0 && !pattern.starts_with('*') && found != 0 {
            return false;
        }
        cursor += found + part.len();
    }
    pattern.ends_with('*')
        || parts
            .last()
            .is_some_and(|part| value[cursor..].ends_with(part))
}
