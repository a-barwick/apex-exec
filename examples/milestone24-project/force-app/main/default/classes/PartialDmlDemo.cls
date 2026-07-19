public class PartialDmlDemo {
    public static void run() {
        M24Row__c first = new M24Row__c();
        first.Name = 'First';
        first.Amount__c = 1;
        M24Row__c invalid = new M24Row__c();
        invalid.Amount__c = 2;
        M24Row__c third = new M24Row__c();
        third.Name = 'Third';
        third.Amount__c = 3;

        List<Database.SaveResult> results = Database.insert(
            new List<M24Row__c>{first, invalid, third},
            false
        );
        System.debug(results.size());
        System.debug(results[0].isSuccess());
        System.debug(results[1].getErrors()[0].getStatusCode());
        System.debug(results[1].getErrors()[0].getFields()[0]);
        System.debug(first.Id != null);
        System.debug(invalid.Id == null);
        System.debug(third.Id != null);

        M24Row__c updated = new M24Row__c();
        updated.Name = 'Updated';
        updated.Amount__c = 4;
        updated.External_Key__c = 'FIRST';
        M24Row__c inserted = new M24Row__c();
        inserted.Name = 'Inserted';
        inserted.Amount__c = 5;
        inserted.External_Key__c = 'new';
        first.External_Key__c = 'first';
        update first;

        List<Database.UpsertResult> upserts = Database.upsert(
            new List<M24Row__c>{updated, inserted},
            M24Row__c.Fields.External_Key__c,
            false
        );
        System.debug(upserts[0].isCreated());
        System.debug(upserts[1].isCreated());
        System.debug([SELECT COUNT() FROM M24Row__c]);
    }
}
