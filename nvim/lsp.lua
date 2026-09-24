local source = debug.getinfo(1, "S").source:sub(2)
local project = vim.fn.fnamemodify(source, ":h:h")

vim.opt.runtimepath:prepend(project .. "/nvim")
vim.filetype.add({ extension = { mf = "mf" } })

vim.lsp.config("mkd", {
  cmd = { project .. "/target/debug/mkd-lsp" },
  filetypes = { "mf" },
  root_markers = { "main.mf", ".git" },
  single_file_support = true,
})
vim.lsp.enable("mkd")

-- 为 mkd 客户端自动开启补全；Neovim 0.11+ 不再默认弹出 LSP 候选。
local doc_group = vim.api.nvim_create_augroup("mkd-completion-doc", { clear = true })
vim.api.nvim_create_autocmd("LspAttach", {
  callback = function(event)
    local client = vim.lsp.get_client_by_id(event.data.client_id)
    if not client or client.name ~= "mkd" then
      return
    end
    vim.bo[event.buf].completeopt = "menuone,noselect,popup"
    vim.lsp.completion.enable(true, client.id, event.buf, { autotrigger = true })
    -- 有些配置虽保留了 info 与 popup，却不会自动创建预览浮窗。
    vim.api.nvim_clear_autocmds({ group = doc_group, buffer = event.buf })
    vim.api.nvim_create_autocmd("CompleteChanged", {
      group = doc_group,
      buffer = event.buf,
      callback = function()
        local item = vim.v.event.completed_item or {}
        if vim.tbl_get(item, "user_data", "nvim", "lsp", "client_id") ~= client.id then
          return
        end
        local selected = vim.fn.complete_info({ "selected" }).selected
        if selected >= 0 and item.info and item.info ~= "" then
          vim.api.nvim__complete_set(selected, { info = item.info })
        end
      end,
    })
  end,
})
