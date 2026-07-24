public class M28C1IdOracle {
    public static void run() {
        String accountPrefix = Schema.Account.SObjectType.getDescribe().getKeyPrefix();
        Id accountId = Id.valueOf(accountPrefix + '000000000000');
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|account|' +
            accountId.getSObjectType().getDescribe().getName()
        );

        Id absent;
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|null|' +
            (absent?.getSObjectType() == null)
        );

        Boolean unknownCaught = false;
        try {
            Id unknownId = Id.valueOf('999000000000001AAA');
            unknownId.getSObjectType();
        } catch (SObjectException error) {
            unknownCaught = true;
        }
        System.debug('APEX_EXEC_ORACLE_VALUE|unknown|' + unknownCaught);
    }
}
