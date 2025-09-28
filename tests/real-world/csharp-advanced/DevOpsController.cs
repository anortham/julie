using CoA.Intranet.Client.Models.Cms;
using CoA.Intranet.Server.Interfaces;
using Microsoft.AspNetCore.Mvc;
using Microsoft.TeamFoundation.WorkItemTracking.WebApi.Models;

namespace CoA.Intranet.Server.Controllers
{
    [Route("api/[controller]")]
    [ApiController]
    public class DevOpsController(IDevOpsService devOpsService) : ControllerBase
    {
        [HttpGet("{workItemId}")]
        public async Task<WorkItem> GetWorkItem(int workItemId)
        {
            return await devOpsService.GetWorkItemAsync(workItemId);
        }

        [HttpPost("deny/{workItemId}")]
        public async Task<WorkItem?> DenyWorkItem(int workItemId, [FromBody] DenialRequest request)
        {
            return await devOpsService.DenyWorkItemAsync(workItemId, request.DenialReason);
        }

        [HttpGet("approve/{workItemId}")]
        public async Task<WorkItem?> ApproveWorkItem(int workItemId)
        {
            return await devOpsService.ApproveWorkItemAsync(workItemId);
        }

        [HttpPost("enterprise-data-story")]
        public async Task<WorkItem> CreateEnterpriseDataStory([FromBody] EnterpriseDataForm form)
        {
            return await devOpsService.CreateEnterpriseDataStoryAsync(form);
        }
    }

    public class DenialRequest
    {
        public string DenialReason { get; set; } = string.Empty;
    }
}