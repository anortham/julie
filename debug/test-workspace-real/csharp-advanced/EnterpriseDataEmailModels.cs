namespace CoA.Intranet.Server.Models.Email;

public class PrimaryDataSponsorEmailModel
{
    public string ShortDescription { get; set; } = string.Empty;
    public string LongDescription { get; set; } = string.Empty;
    public string WorkItemUrl { get; set; } = string.Empty;
    public string AttachmentsHtml { get; set; } = string.Empty;
}

public class SecondaryDataSponsorEmailModel
{
    public string ShortDescription { get; set; } = string.Empty;
    public string LongDescription { get; set; } = string.Empty;
    public string AttachmentsHtml { get; set; } = string.Empty;
}

public class RequestorConfirmationEmailModel
{
    public string ShortDescription { get; set; } = string.Empty;
    public string LongDescription { get; set; } = string.Empty;
    public int? WorkItemId { get; set; }
}

public class RiskManagementApprovalEmailModel
{
    public int WorkItemId { get; set; }
    public string WorkItemUrl { get; set; } = string.Empty;
}

public class RequestDenialEmailModel
{
    public int WorkItemId { get; set; }
    public string ShortDescription { get; set; } = string.Empty;
    public string LongDescription { get; set; } = string.Empty;
    public string DenialReason { get; set; } = string.Empty;
}

public class DataTeamNotificationEmailModel
{
    public string ShortDescription { get; set; } = string.Empty;
    public string LongDescription { get; set; } = string.Empty;
    public string DevOpsUrl { get; set; } = string.Empty;
    public int? WorkItemId { get; set; }
}

public class RequestApprovalEmailModel
{
    public int WorkItemId { get; set; }
    public string ShortDescription { get; set; } = string.Empty;
    public string LongDescription { get; set; } = string.Empty;
}