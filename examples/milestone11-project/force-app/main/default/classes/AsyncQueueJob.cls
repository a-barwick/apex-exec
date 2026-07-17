public class AsyncQueueJob implements Queueable {
    public String label;

    public AsyncQueueJob(String label) {
        this.label = label;
    }

    public void execute(QueueableContext context) {
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'Queue';
        invoice.Amount__c = 10;
        invoice.Status__c = 'queue:' + label;
        insert invoice;
    }
}
