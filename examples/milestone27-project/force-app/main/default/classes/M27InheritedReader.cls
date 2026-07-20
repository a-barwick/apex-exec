public inherited sharing class M27InheritedReader {
    public static Integer countRows() {
        return [SELECT COUNT() FROM M27Row__c];
    }
}
