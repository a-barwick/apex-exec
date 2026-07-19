@IsTest
private class M23QueryFidelityTest {
    @IsTest
    static void querySliceMatchesSalesforce() {
        M23Region__c region = new M23Region__c();
        region.Name = 'West';
        insert region;

        M23Customer__c customer = new M23Customer__c();
        customer.Name = 'Acme';
        customer.Region__c = region.Id;
        insert customer;

        M23Invoice__c first = new M23Invoice__c();
        first.Name = 'I-1';
        first.Amount__c = 10;
        first.Status__c = 'Open';
        first.Customer__c = customer.Id;
        M23Invoice__c second = new M23Invoice__c();
        second.Name = 'I-2';
        second.Amount__c = 20;
        second.Status__c = 'Open';
        second.Customer__c = customer.Id;
        M23Invoice__c third = new M23Invoice__c();
        third.Name = 'I-3';
        third.Amount__c = 30;
        third.Status__c = 'Closed';
        third.Customer__c = customer.Id;
        insert new List<M23Invoice__c>{first, second, third};

        System.assertEquals(
            3,
            [SELECT COUNT() FROM M23Invoice__c WHERE CreatedDate = TODAY]
        );
        List<M23Customer__c> parents = [
            SELECT Id,
                (SELECT Id, Name FROM M23Invoices__r ORDER BY Amount__c DESC LIMIT 1)
            FROM M23Customer__c
            WHERE Name = 'Acme'
        ];
        System.assertEquals(1, parents[0].M23Invoices__r.size());
        System.assertEquals('I-3', parents[0].M23Invoices__r[0].Name);

        List<M23Invoice__c> related = [
            SELECT Id, Customer__r.Region__r.Name
            FROM M23Invoice__c
            WHERE Name = 'I-1'
        ];
        System.assertEquals('West', related[0].Customer__r.Region__r.Name);

        List<AggregateResult> groups = [
            SELECT Status__c, COUNT(Id) total
            FROM M23Invoice__c
            GROUP BY Status__c
            HAVING COUNT(Id) > 1
        ];
        System.assertEquals(1, groups.size());
        System.assertEquals(2, (Integer)groups[0].get('total'));

        String status = 'Open';
        List<M23Invoice__c> dynamicRows = Database.query(
            'SELECT Id FROM M23Invoice__c WHERE Status__c = :status ORDER BY Name'
        );
        System.assertEquals(2, dynamicRows.size());
        System.assertEquals(
            3,
            Database.countQuery(
                'SELECT COUNT() FROM M23Invoice__c WHERE CreatedDate = TODAY'
            )
        );

        Test.startTest();
        Database.executeBatch(new M23InvoiceBatch(), 1);
        Test.stopTest();
        System.assertEquals(
            2,
            Database.countQuery(
                'SELECT COUNT() FROM M23Invoice__c WHERE Status__c = \'Processed\''
            )
        );
    }
}
