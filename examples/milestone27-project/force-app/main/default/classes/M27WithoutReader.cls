public without sharing class M27WithoutReader {
    public static Integer directCount() {
        return [SELECT COUNT() FROM M27Row__c];
    }

    public static Integer inheritedCount() {
        return M27InheritedReader.countRows();
    }
}
