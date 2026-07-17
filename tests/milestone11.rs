use apex_exec::{
    check, execute, project,
    runtime::{AsyncJobKind, AsyncStage, Interpreter, RecordingHost},
};

const EXAMPLE: &str = "examples/milestone11-project";

#[test]
fn queueable_future_batch_and_scheduled_jobs_drain_in_fifo_order() {
    let output = execute(
        r#"
        public class QueueWork implements Queueable {
            public String message;
            public Set<String> tags;
            public Map<String,String> labels;

            public QueueWork(String message) {
                this.message = message;
                this.tags = new Set<String>{message};
                this.labels = new Map<String,String>{'key' => message};
            }

            public void execute(QueueableContext context) {
                System.debug('queue:' + message);
                System.debug(tags.size());
                System.debug(labels.get('key'));
                System.debug(context.getJobId().to15().startsWith('707'));
                System.debug(System.isQueueable());
            }
        }

        public class FutureWork {
            @future
            public static void run(List<String> messages, Blob bytes) {
                System.debug('future:' + messages.get(0));
                System.debug(bytes.size());
                System.debug(System.isFuture());
            }
        }

        public class BatchWork implements Database.Batchable<Integer> {
            public List<Integer> start(Database.BatchableContext context) {
                System.debug(context.getJobId().to15().startsWith('707'));
                return new List<Integer>{1, 2, 3};
            }

            public void execute(Database.BatchableContext context, List<Integer> scope) {
                System.debug('batch:' + scope.size());
                System.debug(System.isBatch());
            }

            public void finish(Database.BatchableContext context) {
                System.debug('batch:finish');
            }
        }

        public class ScheduledWork implements Schedulable {
            public void execute(SchedulableContext context) {
                System.debug(context.getTriggerId().to15().startsWith('707'));
                System.debug(System.isScheduled());
                System.debug('scheduled');
            }
        }

        QueueWork queue = new QueueWork('snapshot');
        List<String> messages = new List<String>{'snapshot'};

        Test.startTest();
        Id queueId = System.enqueueJob(queue);
        FutureWork.run(messages, Blob.valueOf('data'));
        Id batchId = Database.executeBatch(new BatchWork(), 2);
        Id scheduleId = System.schedule(
            'nightly',
            '0 0 0 * * ? 2099',
            new ScheduledWork()
        );
        queue.message = 'mutated';
        queue.tags.add('mutated');
        queue.labels.put('key', 'mutated');
        messages.set(0, 'mutated');
        Test.stopTest();

        System.debug(queueId.to15().startsWith('707'));
        System.debug(batchId.to15().startsWith('707'));
        System.debug(scheduleId.to15().startsWith('707'));
        "#,
    )
    .unwrap();

    assert_eq!(
        output,
        [
            "queue:snapshot",
            "1",
            "snapshot",
            "true",
            "true",
            "future:snapshot",
            "4",
            "true",
            "true",
            "batch:2",
            "true",
            "batch:1",
            "true",
            "batch:finish",
            "true",
            "true",
            "scheduled",
            "true",
            "true",
            "true",
        ]
    );
}

#[test]
fn chained_queueables_drain_to_quiescence_and_record_parent_ids() {
    let checked = check(
        r#"
        public class ChainedWork implements Queueable {
            public Integer depth;

            public ChainedWork(Integer depth) {
                this.depth = depth;
            }

            public void execute(QueueableContext context) {
                System.debug(depth);
                if (depth < 2) {
                    System.enqueueJob(new ChainedWork(depth + 1));
                }
            }
        }

        Test.startTest();
        System.enqueueJob(new ChainedWork(0));
        Test.stopTest();
        "#,
    )
    .unwrap();
    let mut host = RecordingHost::default();
    let output = Interpreter::with_host(&mut host).execute(&checked).unwrap();
    let queued = host
        .async_events()
        .iter()
        .filter(|event| event.stage == AsyncStage::Queued)
        .collect::<Vec<_>>();

    assert_eq!(output, ["0", "1", "2"]);
    assert_eq!(queued.len(), 3);
    assert_eq!(queued[0].parent_job_id, None);
    assert_eq!(
        queued[1].parent_job_id.as_deref(),
        Some(queued[0].job_id.as_str())
    );
    assert_eq!(
        queued[2].parent_job_id.as_deref(),
        Some(queued[1].job_id.as_str())
    );
    assert_eq!(
        host.async_events()
            .iter()
            .filter(|event| event.stage == AsyncStage::Completed)
            .count(),
        3
    );
}

#[test]
fn empty_batch_uses_the_default_scope_and_still_finishes() {
    let output = execute(
        r#"
        public class EmptyBatch implements Database.Batchable<Integer> {
            public List<Integer> start(Database.BatchableContext context) {
                return new List<Integer>();
            }
            public void execute(Database.BatchableContext context, List<Integer> scope) {
                System.debug('unexpected');
            }
            public void finish(Database.BatchableContext context) {
                System.debug('finished');
            }
        }

        Test.startTest();
        Database.executeBatch(new EmptyBatch());
        Test.stopTest();
        "#,
    )
    .unwrap();

    assert_eq!(output, ["finished"]);
}

#[test]
fn async_work_never_runs_in_the_background() {
    let checked = check(
        r#"
        public class DeferredWork implements Queueable {
            public void execute(QueueableContext context) {
                System.debug('ran');
            }
        }
        System.enqueueJob(new DeferredWork());
        "#,
    )
    .unwrap();
    let mut host = RecordingHost::default();
    let output = Interpreter::with_host(&mut host).execute(&checked).unwrap();

    assert!(output.is_empty());
    assert_eq!(host.async_events().len(), 1);
    assert_eq!(host.async_events()[0].kind, AsyncJobKind::Queueable);
    assert_eq!(host.async_events()[0].stage, AsyncStage::Queued);
}

#[test]
fn async_lifecycle_events_are_deterministic_and_report_failures() {
    let checked = check(
        r#"
        public class BrokenWork implements Queueable {
            public void execute(QueueableContext context) {
                Integer failure = 1 / 0;
            }
        }
        Test.startTest();
        System.enqueueJob(new BrokenWork());
        Test.stopTest();
        "#,
    )
    .unwrap();
    let mut host = RecordingHost::default();
    let error = Interpreter::with_host(&mut host)
        .execute(&checked)
        .unwrap_err();

    assert_eq!(error.exception_type.as_deref(), Some("MathException"));
    assert_eq!(
        host.async_events()
            .iter()
            .map(|event| event.stage)
            .collect::<Vec<_>>(),
        [AsyncStage::Queued, AsyncStage::Started, AsyncStage::Failed]
    );
    assert!(
        host.async_events()
            .windows(2)
            .all(|events| events[0].job_id == events[1].job_id)
    );
}

#[test]
fn checker_and_runtime_reject_invalid_async_boundaries_explicitly() {
    let error = check(
        r#"
        public class BadQueue implements Queueable {
            public static void execute(QueueableContext context) {}
        }
        "#,
    )
    .unwrap_err();
    assert!(error.message.contains("Queueable requires"));

    let error = check(
        r#"
        public class BadFuture {
            @future
            public static Integer run(String value) {
                return 1;
            }
        }
        "#,
    )
    .unwrap_err();
    assert!(error.message.contains("@future"));

    let error = check(
        r#"
        public class Plain {}
        System.enqueueJob(new Plain());
        "#,
    )
    .unwrap_err();
    assert!(error.message.contains("does not implement Queueable"));

    let error = execute(
        r#"
        public class TinyBatch implements Database.Batchable<Integer> {
            public List<Integer> start(Database.BatchableContext context) {
                return new List<Integer>();
            }
            public void execute(Database.BatchableContext context, List<Integer> scope) {}
            public void finish(Database.BatchableContext context) {}
        }
        Database.executeBatch(new TinyBatch(), 0);
        "#,
    )
    .unwrap_err();
    assert_eq!(error.exception_type.as_deref(), Some("AsyncException"));
    assert!(error.message.contains("scope size"));

    let error = execute(
        r#"
        public class NullQueue implements Queueable {
            public void execute(QueueableContext context) {}
        }
        NullQueue job = null;
        System.enqueueJob(job);
        "#,
    )
    .unwrap_err();
    assert_eq!(error.exception_type.as_deref(), Some("AsyncException"));
    assert!(error.message.contains("non-null"));

    let error = execute(
        r#"
        public class InvalidPayload implements Queueable {
            public HttpRequest request;
            public InvalidPayload() {
                request = new HttpRequest();
            }
            public void execute(QueueableContext context) {}
        }
        System.enqueueJob(new InvalidPayload());
        "#,
    )
    .unwrap_err();
    assert_eq!(error.exception_type.as_deref(), Some("AsyncException"));
    assert!(error.message.contains("non-serializable platform value"));

    let error = execute(
        r#"
        public class CronWork implements Schedulable {
            public void execute(SchedulableContext context) {}
        }
        System.schedule('bad cron', '0 0', new CronWork());
        "#,
    )
    .unwrap_err();
    assert_eq!(
        error.exception_type.as_deref(),
        Some("IllegalArgumentException")
    );
    assert!(error.message.contains("7 fields"));
}

#[test]
fn milestone11_example_runs_every_async_form_with_full_production_coverage() {
    let compilation = project::compile(EXAMPLE).unwrap();
    let report = apex_exec::test_runner::run(
        &compilation,
        &apex_exec::test_runner::TestOptions {
            filter: None,
            jobs: 2,
        },
    )
    .unwrap();

    assert!(report.is_success());
    assert_eq!(report.tests.len(), 2);
    assert_eq!(report.coverage.covered_lines, report.coverage.total_lines);
    assert_eq!(
        report.coverage.covered_branches,
        report.coverage.total_branches
    );
}
