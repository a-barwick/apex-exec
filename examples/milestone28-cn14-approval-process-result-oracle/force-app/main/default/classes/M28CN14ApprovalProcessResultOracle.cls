public class M28CN14ApprovalProcessResultOracle {
    private static final Id ENTITY_ID = '001000000000001AAA';
    private static final Id WORKITEM_ID = '04i000000000001AAA';

    private static Approval.ProcessResult successResult() {
        return (Approval.ProcessResult) JSON.deserialize(
            '{"success":true,"entityId":"' + ENTITY_ID +
            '","instanceStatus":"Approved","newWorkitemIds":["' +
            WORKITEM_ID + '"]}',
            Approval.ProcessResult.class
        );
    }

    private static Approval.ProcessResult failureResult() {
        return (Approval.ProcessResult) JSON.deserialize(
            '{"success":false,"entityId":"' + ENTITY_ID +
            '","errors":[{"message":"No applicable approval process was found.",' +
            '"statusCode":"NO_APPLICABLE_PROCESS"}]}',
            Approval.ProcessResult.class
        );
    }

    private static Approval.ProcessResult hiddenStateResult() {
        return (Approval.ProcessResult) JSON.deserialize(
            '{"actorIds":["005000000000001AAA"],"entityId":"' +
            ENTITY_ID + '","errors":null,"instanceId":' +
            '"04g000000000001AAA","instanceStatus":null,' +
            '"newWorkitemIds":null}',
            Approval.ProcessResult.class
        );
    }

    public static void run() {
        Approval.ProcessResult success = successResult();
        Approval.ProcessResult failure = failureResult();
        Approval.ProcessResult hiddenState = hiddenStateResult();
        Approval.ProcessResult defaults = (Approval.ProcessResult)
            JSON.deserialize('{}', Approval.ProcessResult.class);
        List<Approval.ProcessResult> results =
            new List<Approval.ProcessResult>{ success, failure };
        List<Approval.ProcessResult> typedResults =
            (List<Approval.ProcessResult>) JSON.deserialize(
                JSON.serialize(results),
                List<Approval.ProcessResult>.class
            );
        Approval.ProcessResult prettyRoundTrip =
            (Approval.ProcessResult) JSON.deserialize(
                JSON.serializePretty(failure),
                Approval.ProcessResult.class
            );

        System.debug(
            'APEX_EXEC_ORACLE_VALUE|type|' +
            Approval.ProcessResult.class.getName()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|listSize|' + results.size()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|success|' + success.isSuccess()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|entityId|' + success.getEntityId()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|successErrorsNull|' +
            (success.getErrors() == null)
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|instanceStatus|' +
            success.getInstanceStatus()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|workitems|' +
            success.getNewWorkitemIds().size()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|workitemId|' +
            success.getNewWorkitemIds()[0]
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|failureSuccess|' + failure.isSuccess()
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
            'APEX_EXEC_ORACLE_VALUE|failureWorkitems|' +
            failure.getNewWorkitemIds().size()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|defaultSuccess|' + defaults.isSuccess()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|defaultEntityNull|' +
            (defaults.getEntityId() == null)
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|defaultErrorsNull|' +
            (defaults.getErrors() == null)
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|defaultStatusNull|' +
            (defaults.getInstanceStatus() == null)
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|defaultWorkitems|' +
            defaults.getNewWorkitemIds().size()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|hiddenCompact|' +
            JSON.serialize(hiddenState)
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|hiddenErrorsNull|' +
            (hiddenState.getErrors() == null)
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|hiddenWorkitems|' +
            hiddenState.getNewWorkitemIds().size()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|hiddenSuccess|' +
            hiddenState.isSuccess()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|typedListSize|' + typedResults.size()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|typedListFailureStatus|' +
            typedResults[1].getErrors()[0].getStatusCode()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|compact|' + JSON.serialize(success)
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|listCompact|' + JSON.serialize(results)
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|pretty|' +
            JSON.serialize(JSON.serializePretty(success))
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|listPretty|' +
            JSON.serialize(JSON.serializePretty(results))
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|defaultCompact|' +
            JSON.serialize(defaults)
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|prettyStatus|' +
            prettyRoundTrip.getErrors()[0].getStatusCode()
        );
    }
}
