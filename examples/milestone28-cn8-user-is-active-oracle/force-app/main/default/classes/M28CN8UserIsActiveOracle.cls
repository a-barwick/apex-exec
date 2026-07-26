public class M28CN8UserIsActiveOracle {
    public static void run() {
        User localUser = new User(IsActive = true);
        List<ApexEmailNotification> notifications = [
            SELECT Email, UserId
            FROM ApexEmailNotification
            WHERE Email != NULL OR User.IsActive = TRUE
        ];

        System.debug(
            'APEX_EXEC_ORACLE_VALUE|assignedValue|' +
            localUser.IsActive
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|fieldName|' +
            Schema.User.IsActive.getDescribe().getName()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|fieldType|' +
            Schema.User.IsActive.getDescribe().getType()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|relationshipFilterExecuted|' +
            (notifications.size() >= 0)
        );
    }
}
