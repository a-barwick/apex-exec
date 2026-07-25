public class M28CN6GeneratedShareOracle {
    public static void run() {
        M28CN6Share__Share share = new M28CN6Share__Share(
            AccessLevel = 'Edit',
            RowCause = Schema.M28CN6Share__Share.RowCause.M28Reason__c
        );
        List<String> accessLevels = new List<String>();
        for (Schema.PicklistEntry entry :
            Schema.M28CN6Share__Share.AccessLevel.getDescribe().getPicklistValues()) {
            accessLevels.add(entry.getValue());
        }
        System.debug('APEX_EXEC_ORACLE_VALUE|shareType|' + share.getSObjectType());
        System.debug('APEX_EXEC_ORACLE_VALUE|accessLevel|' + share.AccessLevel);
        System.debug('APEX_EXEC_ORACLE_VALUE|rowCause|' + share.RowCause);
        System.debug('APEX_EXEC_ORACLE_VALUE|accessLevels|' + String.join(accessLevels, ','));
    }
}
