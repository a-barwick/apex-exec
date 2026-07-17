public class AsyncScheduledJob implements Schedulable {
    public void execute(SchedulableContext context) {
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'Scheduled';
        invoice.Amount__c = 30;
        invoice.Status__c = 'scheduled';
        insert invoice;
    }
}
