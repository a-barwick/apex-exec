public class M28CN13ApprovalLockResultOracle {
    private static final Id RECORD_ID = '001000000000001AAA';

    private static Approval.LockResult successResult() {
        return (Approval.LockResult) JSON.deserialize(
            '{"success":true,"id":"' + RECORD_ID + '"}',
            Approval.LockResult.class
        );
    }

    private static Approval.LockResult failureResult() {
        return (Approval.LockResult) JSON.deserialize(
            '{"success":false,"id":"' + RECORD_ID +
            '","errors":[{"message":"denied","statusCode":' +
            '"INSUFFICIENT_ACCESS_ON_CROSS_REFERENCE_ENTITY"}]}',
            Approval.LockResult.class
        );
    }

    public static void run() {
        Approval.LockResult success = successResult();
        Approval.LockResult failure = failureResult();
        List<Approval.LockResult> results =
            new List<Approval.LockResult>{ success, failure };
        Approval.LockResult prettyRoundTrip =
            (Approval.LockResult) JSON.deserialize(
                JSON.serializePretty(failure),
                Approval.LockResult.class
            );

        System.debug(
            'APEX_EXEC_ORACLE_VALUE|type|' +
            Approval.LockResult.class.getName()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|listSize|' + results.size()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|success|' + success.isSuccess()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|id|' + success.getId()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|errors|' + success.getErrors().size()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|failureSuccess|' + failure.isSuccess()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|failureId|' + failure.getId()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|failureErrors|' +
            failure.getErrors().size()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|failureStatus|' +
            failure.getErrors()[0].getStatusCode()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|failureMessage|' +
            failure.getErrors()[0].getMessage()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|failureFields|' +
            failure.getErrors()[0].getFields().size()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|compact|' + JSON.serialize(success)
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|prettyStatus|' +
            prettyRoundTrip.getErrors()[0].getStatusCode()
        );
    }
}
