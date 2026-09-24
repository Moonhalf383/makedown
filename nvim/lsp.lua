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
vim.api.nvim_create_autocmd("LspAttach", {
  callback = function(event)
    local client = vim.lsp.get_client_by_id(event.data.client_id)
    if client and client.name == "mkd" then
      -- 预选首条候选使 CompleteChanged 立即触发，
      -- 否则 noselect 下预览窗口永远不会显示候选文档。
      vim.bo[event.buf].completeopt = "menuone,popup"
      vim.lsp.completion.enable(true, client.id, event.buf, { autotrigger = true })
    end
  end,
})
