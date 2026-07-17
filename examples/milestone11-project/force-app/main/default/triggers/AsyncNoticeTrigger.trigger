trigger AsyncNoticeTrigger on AsyncNotice__e (after insert) {
    Invoice__c invoice = new Invoice__c();
    invoice.Name = 'Event';
    invoice.Amount__c = 40;
    invoice.Status__c = 'event:' + Trigger.new.get(0).Message__c;
    insert invoice;
}
