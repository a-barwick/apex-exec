public class OracleDemo {
    public static void run() {
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'INV-1';
        invoice.Amount__c = 7;
        insert invoice;

        List<Invoice__c> rows = [
            SELECT Amount__c, Status__c
            FROM Invoice__c
            WHERE Id = :invoice.Id
        ];
        System.debug('APEX_EXEC_ORACLE_VALUE|rowCount|' + rows.size());
        System.debug('APEX_EXEC_ORACLE_VALUE|status|"' + rows[0].Status__c + '"');
        System.debug('loaded invoice');
    }

    public static void fail() {
        Integer impossible = 1 / 0;
        System.debug(impossible);
    }
}
