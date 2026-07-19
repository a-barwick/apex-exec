public class QueryFidelityDemo {
    private static Integer queryTextCalls = 0;

    private static String queryText() {
        queryTextCalls++;
        return 'SELECT Id, Name FROM Invoice__c WHERE Status__c = :status ORDER BY Name';
    }

    public static void run() {
        Region__c region = new Region__c();
        region.Name = 'West';
        insert region;

        Customer__c customer = new Customer__c();
        customer.Name = 'Acme';
        customer.Region__c = region.Id;
        insert customer;

        Invoice__c first = new Invoice__c();
        first.Name = 'I-1';
        first.Amount__c = 10;
        first.Status__c = 'Open';
        first.Customer__c = customer.Id;
        Invoice__c second = new Invoice__c();
        second.Name = 'I-2';
        second.Amount__c = 20;
        second.Status__c = 'Open';
        second.Customer__c = customer.Id;
        Invoice__c third = new Invoice__c();
        third.Name = 'I-3';
        third.Amount__c = 30;
        third.Status__c = 'Closed';
        third.Customer__c = customer.Id;
        insert new List<Invoice__c>{first, second, third};

        Customer__c other = new Customer__c();
        other.Name = 'Other';
        insert other;
        Invoice__c outsider = new Invoice__c();
        outsider.Name = 'Outside';
        outsider.Amount__c = 99;
        outsider.Status__c = 'Archived';
        outsider.Customer__c = other.Id;
        insert outsider;

        List<Invoice__c> today = [
            SELECT Id FROM Invoice__c WHERE CreatedDate = TODAY
        ];
        System.debug(today.size());

        List<Customer__c> parents = [
            SELECT Id, Name,
                (SELECT Id, Name FROM Invoices__r ORDER BY Amount__c DESC LIMIT 1)
            FROM Customer__c
            WHERE Name = 'Acme'
        ];
        System.debug(parents[0].Invoices__r.size());
        System.debug(parents[0].Invoices__r[0].Name);

        List<Invoice__c> related = [
            SELECT Id, Customer__r.Region__r.Name
            FROM Invoice__c
            WHERE Name = 'I-1'
        ];
        System.debug(related[0].Customer__r.Region__r.Name);

        List<AggregateResult> groups = [
            SELECT Status__c, COUNT(Id) total
            FROM Invoice__c
            GROUP BY Status__c
            HAVING COUNT(Id) > 1
            ORDER BY Status__c
        ];
        System.debug(groups.size());
        System.debug((Integer)groups[0].get('total'));

        String status = 'Open';
        List<Invoice__c> dynamicRows = Database.query(queryText());
        System.debug(dynamicRows.size());
        List<Invoice__c> qualifiedRows = System.Database.query(
            'SELECT Id FROM Invoice__c WHERE Status__c = :status'
        );
        System.debug(qualifiedRows.size());
        System.debug(Database.countQuery(
            'SELECT COUNT() FROM Invoice__c WHERE CreatedDate = TODAY'
        ));
        System.debug(queryTextCalls);

        try {
            Database.query('SELECT Missing__c FROM Invoice__c');
        } catch (QueryException error) {
            System.debug(error.getTypeName());
        }
    }
}
