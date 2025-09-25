use crate::extractors::base::{Symbol, SymbolKind, Relationship};
use crate::extractors::razor::RazorExtractor;
use crate::tests::test_utils::init_parser;

fn extract_symbols(code: &str) -> Vec<Symbol> {
    let tree = init_parser(code, "razor");
    let mut extractor = RazorExtractor::new("razor".to_string(), "test.razor".to_string(), code.to_string());
    extractor.extract_symbols(&tree)
}

fn extract_relationships(code: &str, symbols: &[Symbol]) -> Vec<Relationship> {
    let tree = init_parser(code, "razor");
    let mut extractor = RazorExtractor::new("razor".to_string(), "test.razor".to_string(), code.to_string());
    extractor.extract_relationships(&tree, symbols)
}

#[cfg(test)]
mod razor_extractor_tests {
    use super::*;

    #[test]
    fn test_extract_page_directives_model_bindings_and_basic_syntax() {
        let razor_code = r#"@page "/products/{id:int?}"
@model ProductDetailsModel
@using Microsoft.AspNetCore.Authorization
@using MyApp.Models
@inject ILogger<ProductDetailsModel> Logger
@inject IProductService ProductService
@attribute [Authorize]

@{
    ViewData["Title"] = "Product Details";
    Layout = "_Layout";

    var isLoggedIn = User.Identity.IsAuthenticated;
    var productId = Model.ProductId;
    var displayName = Model.Product?.Name ?? "Unknown Product";
}

<div class="product-container">
    <h1>@displayName</h1>

    @if (Model.Product != null)
    {
        <div class="product-details">
            <p>Price: @Model.Product.Price.ToString("C")</p>
            <p>Description: @Html.Raw(Model.Product.Description)</p>

            @if (Model.Product.IsOnSale)
            {
                <span class="sale-badge">ON SALE!</span>
            }
            else
            {
                <span class="regular-price">Regular Price</span>
            }
        </div>
    }
    else
    {
        <div class="error-message">
            <p>Product not found.</p>
            <a href="/products" class="btn btn-primary">Back to Products</a>
        </div>
    }

    @foreach (var review in Model.Reviews ?? Enumerable.Empty<Review>())
    {
        <div class="review">
            <h4>@review.Title</h4>
            <p>Rating: @(new string('★', review.Rating))</p>
            <p>@review.Comment</p>
            <small>By @review.AuthorName on @review.CreatedAt.ToString("MMMM dd, yyyy")</small>
        </div>
    }

    @switch (Model.Product?.Category)
    {
        case "Electronics":
            <partial name="_ElectronicsInfo" model="Model.Product" />
            break;
        case "Clothing":
            <partial name="_ClothingInfo" model="Model.Product" />
            break;
        default:
            <p>Category: @Model.Product?.Category</p>
            break;
    }
</div>

@section Scripts {
    <script>
        window.productId = @productId;

        document.addEventListener('DOMContentLoaded', function() {
            console.log('Product page loaded for ID:', @productId);
        });
    </script>
}

@section Styles {
    <style>
        .product-container {
            max-width: 800px;
            margin: 0 auto;
        }

        .sale-badge {
            color: red;
            font-weight: bnew;
        }
    </style>
}"#;

        let symbols = extract_symbols(razor_code);

        // Page directive
        let page_directive = symbols.iter().find(|s| s.name == "@page");
        assert!(page_directive.is_some());
        assert_eq!(page_directive.unwrap().kind, SymbolKind::Import);
        assert!(page_directive.unwrap().signature.as_ref().unwrap().contains("/products/{id:int?}"));

        // Model directive
        let model_directive = symbols.iter().find(|s| s.name == "@model");
        assert!(model_directive.is_some());
        assert!(model_directive.unwrap().signature.as_ref().unwrap().contains("ProductDetailsModel"));

        // Using directives
        let using_auth = symbols.iter().find(|s| s.name == "Microsoft.AspNetCore.Authorization");
        assert!(using_auth.is_some());
        assert_eq!(using_auth.unwrap().kind, SymbolKind::Import);

        let using_models = symbols.iter().find(|s| s.name == "MyApp.Models");
        assert!(using_models.is_some());

        // Inject directives
        let logger_inject = symbols.iter().find(|s| s.name == "Logger");
        assert!(logger_inject.is_some());
        assert_eq!(logger_inject.unwrap().kind, SymbolKind::Property);
        assert!(logger_inject.unwrap().signature.as_ref().unwrap().contains("@inject ILogger<ProductDetailsModel> Logger"));

        let service_inject = symbols.iter().find(|s| s.name == "ProductService");
        assert!(service_inject.is_some());
        assert!(service_inject.unwrap().signature.as_ref().unwrap().contains("@inject IProductService ProductService"));

        // Attribute directive
        let attribute_directive = symbols.iter().find(|s| s.name == "@attribute");
        assert!(attribute_directive.is_some());
        assert!(attribute_directive.unwrap().signature.as_ref().unwrap().contains("[Authorize]"));

        // Code block variables
        let is_logged_in = symbols.iter().find(|s| s.name == "isLoggedIn");
        assert!(is_logged_in.is_some());
        assert_eq!(is_logged_in.unwrap().kind, SymbolKind::Variable);
        assert!(is_logged_in.unwrap().signature.as_ref().unwrap().contains("User.Identity.IsAuthenticated"));

        let product_id = symbols.iter().find(|s| s.name == "productId");
        assert!(product_id.is_some());
        assert!(product_id.unwrap().signature.as_ref().unwrap().contains("Model.ProductId"));

        let display_name = symbols.iter().find(|s| s.name == "displayName");
        assert!(display_name.is_some());
        assert!(display_name.unwrap().signature.as_ref().unwrap().contains("Model.Product?.Name ?? \"Unknown Product\""));

        // ViewData assignment
        let view_data_title = symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("ViewData[\"Title\"]")));
        assert!(view_data_title.is_some());

        // Section blocks
        let scripts_section = symbols.iter().find(|s| s.name == "Scripts");
        assert!(scripts_section.is_some());
        assert_eq!(scripts_section.unwrap().kind, SymbolKind::Module);
        assert!(scripts_section.unwrap().signature.as_ref().unwrap().contains("@section Scripts"));

        let styles_section = symbols.iter().find(|s| s.name == "Styles");
        assert!(styles_section.is_some());
        assert!(styles_section.unwrap().signature.as_ref().unwrap().contains("@section Styles"));
    }

    #[test]
    fn test_extract_component_parameters_events_and_lifecycle_methods() {
        let razor_code = r#"@namespace MyApp.Components
@inherits ComponentBase
@implements IDisposable
@inject IJSRuntime JSRuntime
@inject NavigationManager Navigation

<div class="user-card @CssClass" @onclick="HandleClick" @onmouseover="HandleMouseOver">
    <img src="@AvatarUrl" alt="@DisplayName" class="avatar" />

    <div class="user-info">
        <h3>@DisplayName</h3>
        <p>@Email</p>

        @if (ShowStatus)
        {
            <span class="status @(IsOnline ? "online" : "offline")">
                @(IsOnline ? "Online" : "Offline")
            </span>
        }

        @if (ChildContent != null)
        {
            <div class="user-actions">
                @ChildContent
            </div>
        }
    </div>

    @if (IsEditing)
    {
        <EditForm Model="EditModel" OnValidSubmit="HandleSubmit">
            <DataAnnotationsValidator />
            <ValidationSummary />

            <div class="form-group">
                <label for="displayName">Display Name:</label>
                <InputText id="displayName" @bind-Value="EditModel.DisplayName" class="form-control" />
                <ValidationMessage For="@(() => EditModel.DisplayName)" />
            </div>

            <div class="form-group">
                <label for="email">Email:</label>
                <InputText id="email" @bind-Value="EditModel.Email" type="email" class="form-control" />
                <ValidationMessage For="@(() => EditModel.Email)" />
            </div>

            <div class="form-actions">
                <button type="submit" class="btn btn-primary" disabled="@IsSubmitting">
                    @if (IsSubmitting)
                    {
                        <span class="spinner-border spinner-border-sm" role="status"></span>
                        <span>Saving...</span>
                    }
                    else
                    {
                        <span>Save Changes</span>
                    }
                </button>
                <button type="button" class="btn btn-secondary" @onclick="CancelEdit">Cancel</button>
            </div>
        </EditForm>
    }
</div>

@code {
    [Parameter] public string? DisplayName { get; set; }
    [Parameter] public string? Email { get; set; }
    [Parameter] public string? AvatarUrl { get; set; }
    [Parameter] public bool IsOnline { get; set; }
    [Parameter] public bool ShowStatus { get; set; } = true;
    [Parameter] public string CssClass { get; set; } = "";
    [Parameter] public RenderFragment? ChildContent { get; set; }
    [Parameter] public EventCallback<MouseEventArgs> OnClick { get; set; }
    [Parameter] public EventCallback<UserUpdatedEventArgs> OnUserUpdated { get; set; }

    [CascadingParameter] public ThemeProvider? Theme { get; set; }
    [CascadingParameter(Name = "UserContext")] public UserContext? UserContext { get; set; }

    private bool IsEditing { get; set; }
    private bool IsSubmitting { get; set; }
    private UserEditModel EditModel { get; set; } = new();
    private IJSObjectReference? jsModule;

    protected override async Task OnInitializedAsync()
    {
        if (string.IsNullOrEmpty(AvatarUrl))
        {
            AvatarUrl = "/images/default-avatar.png";
        }

        EditModel.DisplayName = DisplayName;
        EditModel.Email = Email;

        jsModule = await JSRuntime.InvokeAsync<IJSObjectReference>("import", "./Components/UserCard.razor.js");
    }

    protected override async Task OnParametersSetAsync()
    {
        if (EditModel.DisplayName != DisplayName || EditModel.Email != Email)
        {
            EditModel.DisplayName = DisplayName;
            EditModel.Email = Email;
            StateHasChanged();
        }
    }

    protected override bool ShouldRender()
    {
        return !IsSubmitting;
    }

    protected override async Task OnAfterRenderAsync(bool firstRender)
    {
        if (firstRender && jsModule != null)
        {
            await jsModule.InvokeVoidAsync("initialize", DotNetObjectReference.Create(this));
        }
    }

    private async Task HandleClick(MouseEventArgs args)
    {
        await OnClick.InvokeAsync(args);
    }

    private void HandleMouseOver(MouseEventArgs args)
    {
        // Handle mouse over
    }

    private async Task HandleSubmit()
    {
        IsSubmitting = true;
        StateHasChanged();

        try
        {
            // Simulate API call
            await Task.Delay(1000);

            DisplayName = EditModel.DisplayName;
            Email = EditModel.Email;
            IsEditing = false;

            await OnUserUpdated.InvokeAsync(new UserUpdatedEventArgs
            {
                DisplayName = DisplayName,
                Email = Email
            });
        }
        finally
        {
            IsSubmitting = false;
            StateHasChanged();
        }
    }

    private void CancelEdit()
    {
        IsEditing = false;
        EditModel.DisplayName = DisplayName;
        EditModel.Email = Email;
    }

    [JSInvokable]
    public void OnJSCallback(string message)
    {
        // Handle JavaScript callback
        Console.WriteLine($"JS Callback: {message}");
    }

    public async ValueTask DisposeAsync()
    {
        if (jsModule != null)
        {
            await jsModule.DisposeAsync();
        }
    }

    void IDisposable.Dispose()
    {
        // Cleanup resources
    }
}

@functions {
    private string GetStatusCssClass()
    {
        return IsOnline ? "status-online" : "status-offline";
    }

    private static string FormatLastSeen(DateTime? lastSeen)
    {
        if (!lastSeen.HasValue) return "Never";

        var timeSpan = DateTime.UtcNow - lastSeen.Value;
        return timeSpan.Days > 0 ? $"{timeSpan.Days} days ago" :
               timeSpan.Hours > 0 ? $"{timeSpan.Hours} hours ago" : "Recently";
    }
}

<style>
    .user-card {
        display: flex;
        align-items: center;
        padding: 1rem;
        border: 1px solid #ddd;
        border-radius: 8px;
        cursor: pointer;
        transition: box-shadow 0.2s;
    }

    .user-card:hover {
        box-shadow: 0 2px 8px rgba(0,0,0,0.1);
    }

    .avatar {
        width: 48px;
        height: 48px;
        border-radius: 50%;
        margin-right: 1rem;
    }

    .status.online {
        color: green;
    }

    .status.offline {
        color: #999;
    }
</style>"#;

        let symbols = extract_symbols(razor_code);

        // Namespace directive
        let namespace_directive = symbols.iter().find(|s| s.name == "@namespace");
        assert!(namespace_directive.is_some());
        assert!(namespace_directive.unwrap().signature.as_ref().unwrap().contains("MyApp.Components"));

        // Inherits directive
        let inherits_directive = symbols.iter().find(|s| s.name == "@inherits");
        assert!(inherits_directive.is_some());
        assert!(inherits_directive.unwrap().signature.as_ref().unwrap().contains("ComponentBase"));

        // Implements directive
        let implements_directive = symbols.iter().find(|s| s.name == "@implements");
        assert!(implements_directive.is_some());
        assert!(implements_directive.unwrap().signature.as_ref().unwrap().contains("IDisposable"));

        // Parameters
        let display_name_param = symbols.iter().find(|s| s.name == "DisplayName" && s.signature.as_ref().map_or(false, |sig| sig.contains("[Parameter]")));
        assert!(display_name_param.is_some());
        assert_eq!(display_name_param.unwrap().kind, SymbolKind::Property);
        assert!(display_name_param.unwrap().signature.as_ref().unwrap().contains("[Parameter] public string? DisplayName"));

        let email_param = symbols.iter().find(|s| s.name == "Email" && s.signature.as_ref().map_or(false, |sig| sig.contains("[Parameter]")));
        assert!(email_param.is_some());

        let child_content_param = symbols.iter().find(|s| s.name == "ChildContent" && s.kind == SymbolKind::Property);
        assert!(child_content_param.is_some());
        assert!(child_content_param.unwrap().signature.as_ref().unwrap().contains("RenderFragment? ChildContent"));

        // Event callback parameters
        let on_click_param = symbols.iter().find(|s| s.name == "OnClick");
        assert!(on_click_param.is_some());
        assert!(on_click_param.unwrap().signature.as_ref().unwrap().contains("EventCallback<MouseEventArgs> OnClick"));

        // Cascading parameters
        let theme_param = symbols.iter().find(|s| s.name == "Theme");
        assert!(theme_param.is_some());
        assert!(theme_param.unwrap().signature.as_ref().unwrap().contains("[CascadingParameter] public ThemeProvider? Theme"));

        let user_context_param = symbols.iter().find(|s| s.name == "UserContext");
        assert!(user_context_param.is_some());
        assert!(user_context_param.unwrap().signature.as_ref().unwrap().contains("[CascadingParameter(Name = \"UserContext\")]"));

        // Private fields
        let is_editing = symbols.iter().find(|s| s.name == "IsEditing");
        assert!(is_editing.is_some());
        assert!(is_editing.unwrap().signature.as_ref().unwrap().contains("private bool IsEditing"));

        let edit_model = symbols.iter().find(|s| s.name == "EditModel");
        assert!(edit_model.is_some());
        assert!(edit_model.unwrap().signature.as_ref().unwrap().contains("private UserEditModel EditModel"));


        // Lifecycle methods
        let on_initialized = symbols.iter().find(|s| s.name == "OnInitializedAsync");
        assert!(on_initialized.is_some());
        assert_eq!(on_initialized.unwrap().kind, SymbolKind::Method);
        assert!(on_initialized.unwrap().signature.as_ref().unwrap().contains("protected override async Task OnInitializedAsync()"));

        let on_parameters_set = symbols.iter().find(|s| s.name == "OnParametersSetAsync");
        assert!(on_parameters_set.is_some());
        assert!(on_parameters_set.unwrap().signature.as_ref().unwrap().contains("protected override async Task OnParametersSetAsync()"));

        let should_render = symbols.iter().find(|s| s.name == "ShouldRender");
        assert!(should_render.is_some());
        assert!(should_render.unwrap().signature.as_ref().unwrap().contains("protected override bool ShouldRender()"));

        let on_after_render = symbols.iter().find(|s| s.name == "OnAfterRenderAsync");
        assert!(on_after_render.is_some());
        assert!(on_after_render.unwrap().signature.as_ref().unwrap().contains("protected override async Task OnAfterRenderAsync(bool firstRender)"));

        // Event handlers
        let handle_click = symbols.iter().find(|s| s.name == "HandleClick");
        assert!(handle_click.is_some());
        assert!(handle_click.unwrap().signature.as_ref().unwrap().contains("private async Task HandleClick(MouseEventArgs args)"));

        let handle_submit = symbols.iter().find(|s| s.name == "HandleSubmit");
        assert!(handle_submit.is_some());

        // JSInvokable method
        let js_callback = symbols.iter().find(|s| s.name == "OnJSCallback");
        assert!(js_callback.is_some());
        assert!(js_callback.unwrap().signature.as_ref().unwrap().contains("[JSInvokable]"));

        // Disposal methods
        let dispose_async = symbols.iter().find(|s| s.name == "DisposeAsync");
        assert!(dispose_async.is_some());
        assert!(dispose_async.unwrap().signature.as_ref().unwrap().contains("public async ValueTask DisposeAsync()"));

        let dispose = symbols.iter().find(|s| s.name == "Dispose");
        assert!(dispose.is_some());
        assert!(dispose.unwrap().signature.as_ref().unwrap().contains("void IDisposable.Dispose()"));

        // Functions block
        let get_status_css_class = symbols.iter().find(|s| s.name == "GetStatusCssClass");
        assert!(get_status_css_class.is_some());
        assert!(get_status_css_class.unwrap().signature.as_ref().unwrap().contains("private string GetStatusCssClass()"));

        let format_last_seen = symbols.iter().find(|s| s.name == "FormatLastSeen");
        assert!(format_last_seen.is_some());
        assert!(format_last_seen.unwrap().signature.as_ref().unwrap().contains("private static string FormatLastSeen(DateTime? lastSeen)"));
    }

    #[test]
    fn test_extract_layout_inheritance_sections_and_viewimports() {
        let razor_code = r#"@{
    Layout = "_Layout";
    ViewData["Title"] = "Home Page";
    ViewBag.MetaDescription = "Welcome to our amazing website";
}

@model HomePageModel

<div class="hero-section">
    <h1>@ViewData["Title"]</h1>
    <p class="lead">@Model.WelcomeMessage</p>

    @await Component.InvokeAsync("FeaturedProducts", new { count = 6 })
</div>

<div class="content-sections">
    @foreach (var section in Model.ContentSections)
    {
        <section class="content-section">
            <h2>@section.Title</h2>
            <div class="section-content">
                @Html.Raw(section.Content)
            </div>

            @if (section.HasCallToAction)
            {
                <div class="cta-section">
                    <a href="@section.CallToActionUrl" class="btn btn-primary">
                        @section.CallToActionText
                    </a>
                </div>
            }
        </section>
    }
</div>

@section MetaTags {
    <meta name="description" content="@ViewBag.MetaDescription" />
    <meta property="og:title" content="@ViewData["Title"]" />
    <meta property="og:description" content="@ViewBag.MetaDescription" />
    <meta property="og:image" content="@Url.Action("GetOgImage", "Home")" />
}

@section Scripts {
    <script src="~/js/home.js" asp-append-version="true"></script>
    <script>
        window.homeData = {
            welcomeMessage: '@Html.Raw(Json.Serialize(Model.WelcomeMessage))',
            sectionCount: @Model.ContentSections.Count,
            isAuthenticated: @Json.Serialize(User.Identity.IsAuthenticated)
        };
    </script>

    @{await Html.RenderPartialAsync("_AnalyticsScripts");}
}

@section Styles {
    <link rel="stylesheet" href="~/css/home.css" asp-append-version="true" />
    <style>
        .hero-section {
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 4rem 0;
            text-align: center;
        }

        .content-section {
            margin: 2rem 0;
            padding: 2rem;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }
    </style>
}

@functions {
    private string GetSectionCssClass(string sectionType)
    {
        return sectionType switch
        {
            "featured" => "section-featured",
            "news" => "section-news",
            "testimonials" => "section-testimonials",
            _ => "section-default"
        };
    }

    private async Task<string> GetLocalizedContent(string key)
    {
        // Simulate localization lookup
        await Task.Delay(1);
        return $"Localized: {key}";
    }
}"#;

        let layout_code = r#"@using Microsoft.AspNetCore.Mvc.TagHelpers
@namespace MyApp.Views.Shared
@addTagHelper *, Microsoft.AspNetCore.Mvc.TagHelpers
@addTagHelper *, MyApp.TagHelpers

<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>@ViewData["Title"] - MyApp</title>

    <link rel="stylesheet" href="~/lib/bootstrap/dist/css/bootstrap.min.css" />
    <link rel="stylesheet" href="~/css/site.css" asp-append-version="true" />

    @await RenderSectionAsync("MetaTags", required: false)
    @await RenderSectionAsync("Styles", required: false)
</head>
<body>
    <header>
        <nav class="navbar navbar-expand-sm navbar-toggleable-sm navbar-light bg-white border-bottom box-shadow mb-3">
            <div class="container">
                <a class="navbar-brand" asp-controller="Home" asp-action="Index">MyApp</a>

                <div class="navbar-collapse collapse d-sm-inline-flex justify-content-between">
                    <ul class="navbar-nav flex-grow-1">
                        <li class="nav-item">
                            <a class="nav-link text-dark" asp-controller="Home" asp-action="Index">Home</a>
                        </li>
                        <li class="nav-item">
                            <a class="nav-link text-dark" asp-controller="Products" asp-action="Index">Products</a>
                        </li>
                    </ul>

                    <partial name="_LoginPartial" />
                </div>
            </div>
        </nav>
    </header>

    <div class="container">
        <main role="main" class="pb-3">
            @RenderBody()
        </main>
    </div>

    <footer class="border-top footer text-muted">
        <div class="container">
            &copy; @DateTime.Now.Year - MyApp -
            <a asp-controller="Home" asp-action="Privacy">Privacy</a>
        </div>
    </footer>

    <script src="~/lib/jquery/dist/jquery.min.js"></script>
    <script src="~/lib/bootstrap/dist/js/bootstrap.bundle.min.js"></script>
    <script src="~/js/site.js" asp-append-version="true"></script>

    @await RenderSectionAsync("Scripts", required: false)

    <environment include="Development">
        <script src="~/js/debug.js"></script>
    </environment>
</body>
</html>"#;

        let symbols = extract_symbols(razor_code);

        // Layout assignment
        let layout_assignment = symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("Layout = \"_Layout\"")));
        assert!(layout_assignment.is_some());

        // ViewData assignments
        let title_assignment = symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("ViewData[\"Title\"]")));
        assert!(title_assignment.is_some());

        let meta_description = symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("ViewBag.MetaDescription")));
        assert!(meta_description.is_some());

        // Component invocation
        let component_invoke = symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("Component.InvokeAsync(\"FeaturedProducts\"")));
        assert!(component_invoke.is_some());

        // Sections
        let meta_tags_section = symbols.iter().find(|s| s.name == "MetaTags" && s.signature.as_ref().map_or(false, |sig| sig.contains("@section MetaTags")));
        assert!(meta_tags_section.is_some());
        assert_eq!(meta_tags_section.unwrap().kind, SymbolKind::Module);

        let scripts_section = symbols.iter().find(|s| s.name == "Scripts" && s.signature.as_ref().map_or(false, |sig| sig.contains("@section Scripts")));
        assert!(scripts_section.is_some());

        let styles_section = symbols.iter().find(|s| s.name == "Styles" && s.signature.as_ref().map_or(false, |sig| sig.contains("@section Styles")));
        assert!(styles_section.is_some());

        // Functions
        let get_section_css_class = symbols.iter().find(|s| s.name == "GetSectionCssClass");
        assert!(get_section_css_class.is_some());
        assert!(get_section_css_class.unwrap().signature.as_ref().unwrap().contains("private string GetSectionCssClass(string sectionType)"));

        let get_localized_content = symbols.iter().find(|s| s.name == "GetLocalizedContent");
        assert!(get_localized_content.is_some());
        assert!(get_localized_content.unwrap().signature.as_ref().unwrap().contains("private async Task<string> GetLocalizedContent(string key)"));

        // Test layout parsing separately
        let layout_symbols = extract_symbols(layout_code);

        // Layout directives
        let using_directive = layout_symbols.iter().find(|s| s.name == "Microsoft.AspNetCore.Mvc.TagHelpers");
        assert!(using_directive.is_some());

        let namespace_directive = layout_symbols.iter().find(|s| s.name == "@namespace");
        assert!(namespace_directive.is_some());
        assert!(namespace_directive.unwrap().signature.as_ref().unwrap().contains("MyApp.Views.Shared"));

        let add_tag_helper = layout_symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("@addTagHelper *, Microsoft.AspNetCore.Mvc.TagHelpers")));
        assert!(add_tag_helper.is_some());

        // Render methods
        let render_section_async = layout_symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("RenderSectionAsync(\"MetaTags\"")));
        assert!(render_section_async.is_some());

        let render_body = layout_symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("RenderBody()")));
        assert!(render_body.is_some());
    }

    #[test]
    fn test_extract_two_way_binding_event_handlers_and_form_validation() {
        let razor_code = r#"@page "/contact"
@model ContactFormModel
@inject IEmailService EmailService
@inject IValidator<ContactFormModel> Validator

<div class="contact-form-container">
    <h2>Contact Us</h2>

    <EditForm Model="Model" OnValidSubmit="HandleValidSubmit" OnInvalidSubmit="HandleInvalidSubmit">
        <ObjectGraphDataAnnotationsValidator />
        <ValidationSummary class="text-danger" />

        <div class="form-row">
            <div class="form-group col-md-6">
                <label for="firstName">First Name</label>
                <InputText id="firstName" class="form-control" @bind-Value="Model.FirstName"
                          @bind-Value:event="oninput" placehnewer="Enter first name" />
                <ValidationMessage For="@(() => Model.FirstName)" class="text-danger" />
            </div>

            <div class="form-group col-md-6">
                <label for="lastName">Last Name</label>
                <InputText id="lastName" class="form-control" @bind-Value="Model.LastName"
                          placehnewer="Enter last name" />
                <ValidationMessage For="@(() => Model.LastName)" class="text-danger" />
            </div>
        </div>

        <div class="form-group">
            <label for="email">Email Address</label>
            <InputText id="email" type="email" class="form-control" @bind-Value="Model.Email"
                      @onblur="ValidateEmail" @onfocus="ClearEmailValidation" />
            <ValidationMessage For="@(() => Model.Email)" class="text-danger" />
        </div>

        <div class="form-group">
            <label for="subject">Subject</label>
            <InputSelect id="subject" class="form-control" @bind-Value="Model.Subject"
                        @onchange="HandleSubjectChange">
                <option value="">Select a subject</option>
                <option value="general">General Inquiry</option>
                <option value="support">Technical Support</option>
                <option value="sales">Sales Question</option>
                <option value="feedback">Feedback</option>
            </InputSelect>
            <ValidationMessage For="@(() => Model.Subject)" class="text-danger" />
        </div>

        <div class="form-group">
            <label for="priority">Priority Level</label>
            <InputRadioGroup @bind-Value="Model.Priority" class="priority-group">
                <div class="form-check form-check-inline">
                    <InputRadio Value="@PriorityLevel.Low" id="priorityLow" class="form-check-input" />
                    <label class="form-check-label" for="priorityLow">Low</label>
                </div>
                <div class="form-check form-check-inline">
                    <InputRadio Value="@PriorityLevel.Medium" id="priorityMedium" class="form-check-input" />
                    <label class="form-check-label" for="priorityMedium">Medium</label>
                </div>
                <div class="form-check form-check-inline">
                    <InputRadio Value="@PriorityLevel.High" id="priorityHigh" class="form-check-input" />
                    <label class="form-check-label" for="priorityHigh">High</label>
                </div>
            </InputRadioGroup>
        </div>

        <div class="form-group">
            <div class="form-check">
                <InputCheckbox id="newsletter" class="form-check-input" @bind-Value="Model.SubscribeToNewsletter" />
                <label class="form-check-label" for="newsletter">
                    Subscribe to our newsletter
                </label>
            </div>

            <div class="form-check">
                <InputCheckbox id="terms" class="form-check-input" @bind-Value="Model.AcceptTerms" />
                <label class="form-check-label" for="terms">
                    I accept the <a href="/terms" target="_blank">terms and conditions</a>
                </label>
            </div>
        </div>

        <div class="form-group">
            <label for="message">Message</label>
            <InputTextArea id="message" class="form-control" rows="6" @bind-Value="Model.Message"
                          @oninput="HandleMessageInput" placehnewer="Enter your message..." />
            <ValidationMessage For="@(() => Model.Message)" class="text-danger" />
            <small class="form-text text-muted">
                Character count: @(Model.Message?.Length ?? 0) / @Model.MaxMessageLength
            </small>
        </div>

        <div class="form-group">
            <label for="attachment">Attachment (optional)</label>
            <InputFile id="attachment" class="form-control-file" OnChange="HandleFileSelection"
                      accept=".pdf,.doc,.docx,.txt" multiple />

            @if (SelectedFiles.Any())
            {
                <div class="selected-files mt-2">
                    <h6>Selected Files:</h6>
                    <ul class="list-unstyled">
                        @foreach (var file in SelectedFiles)
                        {
                            <li class="d-flex justify-content-between align-items-center">
                                <span>@file.Name (@file.Size.ToString("N0") bytes)</span>
                                <button type="button" class="btn btn-sm btn-outline-danger"
                                       @onclick="() => RemoveFile(file)">Remove</button>
                            </li>
                        }
                    </ul>
                </div>
            }
        </div>

        <div class="form-actions">
            <button type="submit" class="btn btn-primary" disabled="@(IsSubmitting || !IsFormValid)">
                @if (IsSubmitting)
                {
                    <span class="spinner-border spinner-border-sm" role="status"></span>
                    <span>Sending...</span>
                }
                else
                {
                    <i class="fas fa-paper-plane"></i>
                    <span>Send Message</span>
                }
            </button>

            <button type="button" class="btn btn-secondary ml-2" @onclick="ResetForm">
                Reset Form
            </button>
        </div>
    </EditForm>

    @if (!string.IsNullOrEmpty(SubmissionMessage))
    {
        <div class="alert @(IsSubmissionSuccess ? "alert-success" : "alert-danger") mt-3" role="alert">
            @SubmissionMessage
        </div>
    }
</div>

@code {
    private bool IsSubmitting { get; set; }
    private bool IsFormValid { get; set; }
    private bool IsSubmissionSuccess { get; set; }
    private string? SubmissionMessage { get; set; }
    private List<IBrowserFile> SelectedFiles { get; set; } = new();
    private Timer? validationTimer;

    protected override async Task OnInitializedAsync()
    {
        Model.Priority = PriorityLevel.Medium;
        await ValidateForm();
    }

    private async Task HandleValidSubmit(EditContext editContext)
    {
        IsSubmitting = true;
        StateHasChanged();

        try
        {
            var result = await EmailService.SendContactEmailAsync(Model, SelectedFiles);

            if (result.IsSuccess)
            {
                SubmissionMessage = "Thank you for your message! We'll get back to you soon.";
                IsSubmissionSuccess = true;
                await ResetForm();
            }
            else
            {
                SubmissionMessage = $"Error sending message: {result.ErrorMessage}";
                IsSubmissionSuccess = false;
            }
        }
        catch (Exception ex)
        {
            SubmissionMessage = "An unexpected error occurred. Please try again.";
            IsSubmissionSuccess = false;
        }
        finally
        {
            IsSubmitting = false;
            StateHasChanged();
        }
    }

    private void HandleInvalidSubmit(EditContext editContext)
    {
        SubmissionMessage = "Please correct the errors below and try again.";
        IsSubmissionSuccess = false;
    }

    private async Task ValidateEmail()
    {
        if (!string.IsNullOrEmpty(Model.Email))
        {
            var isValid = await EmailService.ValidateEmailAddressAsync(Model.Email);
            if (!isValid)
            {
                // Add custom validation error
            }
        }
    }

    private void ClearEmailValidation()
    {
        // Clear any custom validation messages
    }

    private async Task HandleSubjectChange(ChangeEventArgs e)
    {
        Model.Subject = e.Value?.ToString();
        await ValidateForm();
    }

    private async Task HandleMessageInput(ChangeEventArgs e)
    {
        Model.Message = e.Value?.ToString();

        // Debounce validation
        validationTimer?.Dispose();
        validationTimer = new Timer(async _ => await ValidateForm(), null, 500, Timeout.Infinite);
    }

    private async Task HandleFileSelection(InputFileChangeEventArgs e)
    {
        SelectedFiles.Clear();

        foreach (var file in e.GetMultipleFiles(maxAllowedFiles: 5))
        {
            if (file.Size <= 10 * 1024 * 1024) // 10MB limit
            {
                SelectedFiles.Add(file);
            }
        }

        StateHasChanged();
    }

    private void RemoveFile(IBrowserFile file)
    {
        SelectedFiles.Remove(file);
        StateHasChanged();
    }

    private async Task ResetForm()
    {
        Model = new ContactFormModel { Priority = PriorityLevel.Medium };
        SelectedFiles.Clear();
        SubmissionMessage = null;
        await ValidateForm();
        StateHasChanged();
    }

    private async Task ValidateForm()
    {
        var validationResult = await Validator.ValidateAsync(Model);
        IsFormValid = validationResult.IsValid && Model.AcceptTerms;
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            validationTimer?.Dispose();
        }
        base.Dispose(disposing);
    }
}"#;

        let symbols = extract_symbols(razor_code);


        // Two-way binding
        let first_name_binding = symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("@bind-Value=\"Model.FirstName\"")));
        assert!(first_name_binding.is_some());

        let email_binding = symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("@bind-Value=\"Model.Email\"")));
        assert!(email_binding.is_some());

        // Event binding with custom event
        let input_binding = symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("@bind-Value:event=\"oninput\"")));
        assert!(input_binding.is_some());

        // Event handlers
        let validate_email = symbols.iter().find(|s| s.name == "ValidateEmail");
        assert!(validate_email.is_some());
        assert!(validate_email.unwrap().signature.as_ref().unwrap().contains("private async Task ValidateEmail()"));

        let handle_subject_change = symbols.iter().find(|s| s.name == "HandleSubjectChange");
        assert!(handle_subject_change.is_some());
        assert!(handle_subject_change.unwrap().signature.as_ref().unwrap().contains("private async Task HandleSubjectChange(ChangeEventArgs e)"));

        let handle_file_selection = symbols.iter().find(|s| s.name == "HandleFileSelection");
        assert!(handle_file_selection.is_some());
        assert!(handle_file_selection.unwrap().signature.as_ref().unwrap().contains("private async Task HandleFileSelection(InputFileChangeEventArgs e)"));

        // Form submission handlers
        let handle_valid_submit = symbols.iter().find(|s| s.name == "HandleValidSubmit");
        assert!(handle_valid_submit.is_some());
        assert!(handle_valid_submit.unwrap().signature.as_ref().unwrap().contains("private async Task HandleValidSubmit(EditContext editContext)"));

        let handle_invalid_submit = symbols.iter().find(|s| s.name == "HandleInvalidSubmit");
        assert!(handle_invalid_submit.is_some());

        // Private fields
        let is_submitting = symbols.iter().find(|s| s.name == "IsSubmitting");
        assert!(is_submitting.is_some());
        assert!(is_submitting.unwrap().signature.as_ref().unwrap().contains("private bool IsSubmitting"));

        let selected_files = symbols.iter().find(|s| s.name == "SelectedFiles");
        assert!(selected_files.is_some());
        assert!(selected_files.unwrap().signature.as_ref().unwrap().contains("private List<IBrowserFile> SelectedFiles"));

        let validation_timer = symbols.iter().find(|s| s.name == "validationTimer");
        assert!(validation_timer.is_some());

        // Lifecycle and utility methods
        let on_initialized_async = symbols.iter().find(|s| s.name == "OnInitializedAsync");
        assert!(on_initialized_async.is_some());

        let reset_form = symbols.iter().find(|s| s.name == "ResetForm");
        assert!(reset_form.is_some());

        let validate_form = symbols.iter().find(|s| s.name == "ValidateForm");
        assert!(validate_form.is_some());

        let remove_file = symbols.iter().find(|s| s.name == "RemoveFile" && s.kind == SymbolKind::Method);
        assert!(remove_file.is_some());
        assert!(remove_file.unwrap().signature.as_ref().unwrap().contains("private void RemoveFile(IBrowserFile file)"));

        // Disposal
        let dispose = symbols.iter().find(|s| s.name == "Dispose");
        assert!(dispose.is_some());
        assert!(dispose.unwrap().signature.as_ref().unwrap().contains("protected override void Dispose(bool disposing)"));
    }

    #[test]
    fn test_infer_types_from_razor_code_blocks_and_csharp_syntax() {
        let razor_code = r#"@page "/dashboard"
@model DashboardModel
@inject IUserService UserService
@inject ILogger<Dashboard> Logger

@code {
    private bool IsLoading { get; set; } = true;
    private string? ErrorMessage { get; set; }
    private List<UserData> Users { get; set; } = new();
    private Timer? RefreshTimer { get; set; }

    protected override async Task OnInitializedAsync()
    {
        await LoadUsers();
        StartAutoRefresh();
    }

    private async Task LoadUsers()
    {
        try
        {
            IsLoading = true;
            Users = await UserService.GetActiveUsersAsync();
        }
        catch (Exception ex)
        {
            ErrorMessage = ex.Message;
            Logger.LogError(ex, "Failed to load users");
        }
        finally
        {
            IsLoading = false;
            StateHasChanged();
        }
    }

    private void StartAutoRefresh()
    {
        RefreshTimer = new Timer(async _ => await LoadUsers(), null, TimeSpan.FromMinutes(5), TimeSpan.FromMinutes(5));
    }
}"#;

        let symbols = extract_symbols(razor_code);
        let types = {
            let tree = init_parser(razor_code, "razor");
            let extractor = RazorExtractor::new("razor".to_string(), "test.razor".to_string(), razor_code.to_string());
            extractor.infer_types(&symbols)
        };

        // Property types
        let is_loading = symbols.iter().find(|s| s.name == "IsLoading");
        assert!(is_loading.is_some());
        assert_eq!(types.get(&is_loading.unwrap().id).unwrap(), "bool");

        let error_message = symbols.iter().find(|s| s.name == "ErrorMessage");
        assert!(error_message.is_some());
        assert_eq!(types.get(&error_message.unwrap().id).unwrap(), "string?");

        let users = symbols.iter().find(|s| s.name == "Users");
        assert!(users.is_some());
        assert_eq!(types.get(&users.unwrap().id).unwrap(), "List<UserData>");

        let refresh_timer = symbols.iter().find(|s| s.name == "RefreshTimer");
        assert!(refresh_timer.is_some());
        assert_eq!(types.get(&refresh_timer.unwrap().id).unwrap(), "Timer?");

        // Method return types
        let on_initialized = symbols.iter().find(|s| s.name == "OnInitializedAsync");
        assert!(on_initialized.is_some());
        assert_eq!(types.get(&on_initialized.unwrap().id).unwrap(), "Task");

        let load_users = symbols.iter().find(|s| s.name == "LoadUsers" && s.kind == SymbolKind::Method);
        assert!(load_users.is_some());
        assert_eq!(types.get(&load_users.unwrap().id).unwrap(), "Task");

        let start_auto_refresh = symbols.iter().find(|s| s.name == "StartAutoRefresh" && s.kind == SymbolKind::Method);
        assert!(start_auto_refresh.is_some());
        assert_eq!(types.get(&start_auto_refresh.unwrap().id).unwrap(), "void");
    }

    #[test]
    fn test_extract_component_relationships_and_dependencies() {
        let razor_code = r#"@inherits LayoutComponentBase
@implements IDisposable
@inject IJSRuntime JSRuntime
@inject IConfiguration Configuration

<div class="app-layout">
    <AppHeader User="@CurrentUser" OnMenuToggle="HandleMenuToggle" />

    <aside class="sidebar @(IsSidebarOpen ? "open" : "closed")">
        <Navigation />
    </aside>

    <main class="main-content">
        @Body

        <AppFooter Version="@AppVersion" />
    </main>

    <ErrorBoundary>
        <ChildContent>
            <NotificationContainer />
        </ChildContent>
        <ErrorContent>
            <div class="error-fallback">
                <h3>Something went wrong</h3>
                <p>Please refresh the page and try again.</p>
            </div>
        </ErrorContent>
    </ErrorBoundary>
</div>

@code {
    [CascadingParameter] public User? CurrentUser { get; set; }

    private bool IsSidebarOpen { get; set; } = true;
    private string AppVersion { get; set; } = "";

    protected override async Task OnInitializedAsync()
    {
        AppVersion = Configuration["AppVersion"] ?? "1.0.0";
        await JSRuntime.InvokeVoidAsync("initializeLayout");
    }

    private void HandleMenuToggle()
    {
        IsSidebarOpen = !IsSidebarOpen;
        StateHasChanged();
    }

    public void Dispose()
    {
        // Cleanup
    }
}"#;

        let symbols = extract_symbols(razor_code);
        let relationships = extract_relationships(razor_code, &symbols);


        // Should find component usage relationships
        assert!(relationships.len() >= 4);

        // Component dependencies (uses relationships)
        let header_usage = relationships.iter().find(|r| {
            r.kind.to_string() == "uses" &&
            symbols.iter().find(|s| &s.id == &r.to_symbol_id).map_or(false, |s| s.name == "AppHeader")
        });
        assert!(header_usage.is_some());

        let navigation_usage = relationships.iter().find(|r| {
            r.kind.to_string() == "uses" &&
            symbols.iter().find(|s| &s.id == &r.to_symbol_id).map_or(false, |s| s.name == "Navigation")
        });
        assert!(navigation_usage.is_some());

        let footer_usage = relationships.iter().find(|r| {
            r.kind.to_string() == "uses" &&
            symbols.iter().find(|s| &s.id == &r.to_symbol_id).map_or(false, |s| s.name == "AppFooter")
        });
        assert!(footer_usage.is_some());

        let notification_usage = relationships.iter().find(|r| {
            r.kind.to_string() == "uses" &&
            symbols.iter().find(|s| &s.id == &r.to_symbol_id).map_or(false, |s| s.name == "NotificationContainer")
        });
        assert!(notification_usage.is_some());
    }
}