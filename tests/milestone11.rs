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

            public QueueWork(String message) {
                this.message = message;
            }

            public void execute(QueueableContext context) {
                System.debug('queue:' + message);
                System.debug(context.getJobId().to15().startsWith('707'));
                System.debug(System.isQueueable());
            }
        }

        public class FutureWork {
            @future
            public static void run(List<String> messages) {
                System.debug('future:' + messages.get(0));
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
        FutureWork.run(messages);
        Id batchId = Database.executeBatch(new BatchWork(), 2);
        Id scheduleId = System.schedule(
            'nightly',
            '0 0 0 * * ? 2099',
            new ScheduledWork()
        );
        queue.message = 'mutated';
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
            "true",
            "true",
            "future:snapshot",
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
