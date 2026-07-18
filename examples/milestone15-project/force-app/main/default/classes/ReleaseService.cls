public class ReleaseService {
    public static Invoice__c prepare(Integer amount) {
        Invoice__c invoice = new Invoice__c();
        invoice.Amount__c = amount;
        invoice.Status__c = classify(amount);
        return invoice;
    }

    public static String classify(Integer amount) {
        if (amount >= 100) {
            return 'priority';
        }
        return 'standard';
    }
}
