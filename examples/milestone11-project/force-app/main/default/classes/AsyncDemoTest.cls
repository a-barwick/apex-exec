@IsTest
private class AsyncDemoTest {
    @TestSetup
    static void seedInvoices() {
        List<Invoice__c> invoices = new List<Invoice__c>();
        for (Integer amount = 1; amount <= 2; amount++) {
            Invoice__c invoice = new Invoice__c();
            invoice.Name = 'Seed';
            invoice.Amount__c = amount;
            invoice.Status__c = 'seed';
            invoices.add(invoice);
        }
        insert invoices;
    }

    @IsTest
    static void drainsEveryAsyncFormDeterministically() {
        AsyncQueueJob queue = new AsyncQueueJob('original');
        AsyncNotice__e notice = new AsyncNotice__e();
        notice.Message__c = 'event';
        List<AsyncNotice__e> notices = new List<AsyncNotice__e>{notice};

        Test.startTest();
        Id queueId = System.enqueueJob(queue);
        AsyncFutureService.createInvoice('future');
        Id batchId = Database.executeBatch(new AsyncInvoiceBatch(), 2);
        Id scheduleId = System.schedule(
            'nightly invoice job',
            '0 0 0 * * ? 2099',
            new AsyncScheduledJob()
        );
        EventBus.publish(notices);
        queue.label = 'mutated after enqueue';
        Test.stopTest();

        System.assert(queueId.to15().startsWith('707'));
        System.assert(batchId.to15().startsWith('707'));
        System.assert(scheduleId.to15().startsWith('707'));
        System.assertEquals(6, [SELECT COUNT() FROM Invoice__c]);
        System.assertEquals(
            4,
            [SELECT COUNT() FROM Invoice__c WHERE Status__c = 'batched']
        );
        System.assertEquals(
            1,
            [SELECT COUNT() FROM Invoice__c WHERE Status__c = 'scheduled']
        );
        System.assertEquals(
            1,
            [SELECT COUNT() FROM Invoice__c WHERE Status__c = 'event:event']
        );
    }

    @IsTest
    static void queuePayloadUsesEnqueueTimeSnapshot() {
        AsyncQueueJob queue = new AsyncQueueJob('original');

        Test.startTest();
        System.enqueueJob(queue);
        queue.label = 'mutated';
        Test.stopTest();

        System.assertEquals(
            1,
            [SELECT COUNT() FROM Invoice__c WHERE Status__c = 'queue:original']
        );
        System.assertEquals(
            0,
            [SELECT COUNT() FROM Invoice__c WHERE Status__c = 'queue:mutated']
        );
    }
}
