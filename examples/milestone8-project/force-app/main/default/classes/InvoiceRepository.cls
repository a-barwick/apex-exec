public class InvoiceRepository {
    public static List<Invoice__c> findHighValue(Integer minimumAmount) {
        return [
            SELECT Id, Name, Amount__c, Customer__r.Name
            FROM Invoice__c
            WHERE Amount__c >= :minimumAmount
            ORDER BY Amount__c DESC
            LIMIT 10
        ];
    }
}
