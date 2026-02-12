(function initAccountUi(root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.AccountUi = api;
  }
}(typeof globalThis !== 'undefined' ? globalThis : this, function factory() {
  function createAccountUi(deps) {
    const {
      query,
      serviceMeta,
      savedTokenMask,
      escHtml,
      deriveTokenInputValue,
      normalizeAccountToken,
      queuePersistSetup,
      deleteAccount,
      log,
      makeId,
    } = deps;

    const makeAccountId = makeId || ((service) => `${service}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`);

    function defaultAccount(service, idx = 0) {
      const base = serviceMeta[service].label;
      return { id: makeAccountId(service), name: `${base} ${idx + 1}`, token: '', hasToken: false };
    }

    function accountFromRow(row) {
      const id = row.dataset.accountId || '';
      const name = row.querySelector('.account-name')?.value?.trim() || '';
      const rawToken = row.querySelector('.account-token')?.value || '';
      const token = normalizeAccountToken({
        rawToken,
        tokenMasked: row.dataset.tokenMasked === '1',
        savedTokenMask,
      });
      const hasToken = row.dataset.hasToken === '1';
      return { id, name, token, hasToken };
    }

    function readAccountsFromDom(service) {
      const list = query(serviceMeta[service].listId);
      const rows = Array.from(list.querySelectorAll('.account-row'));
      return rows.map(accountFromRow);
    }

    function upsertDomTokenState(service, id, hasToken) {
      const list = query(serviceMeta[service].listId);
      const row = Array.from(list.querySelectorAll('.account-row'))
        .find((candidate) => candidate.dataset.accountId === id);
      if (!row) return;
      row.dataset.hasToken = hasToken ? '1' : '0';
      const tokenInput = row.querySelector('.account-token');
      if (tokenInput) {
        tokenInput.placeholder = 'eyJhbG... / sk-...';
        if (hasToken) {
          tokenInput.value = savedTokenMask;
          row.dataset.tokenMasked = '1';
        } else {
          tokenInput.value = '';
          row.dataset.tokenMasked = '0';
        }
      }
    }

    function writeAccountsToDom(service, accounts) {
      const list = query(serviceMeta[service].listId);
      list.innerHTML = '';
      for (const acc of accounts) {
        const row = document.createElement('div');
        row.className = 'account-row';
        row.dataset.accountId = acc.id || makeAccountId(service);
        row.dataset.hasToken = acc.hasToken ? '1' : '0';

        const tokenView = deriveTokenInputValue({
          hasToken: acc.hasToken,
          token: acc.token,
          savedTokenMask,
        });
        row.dataset.tokenMasked = tokenView.tokenMasked ? '1' : '0';

        row.innerHTML = `
          <input class="account-name" type="text" placeholder="表示名" value="${escHtml(acc.name || '')}">
          <input class="account-token" type="text" placeholder="eyJhbG... / sk-..." value="${escHtml(tokenView.tokenValue)}">
          <button class="btn-mini btn-remove-account" type="button">削除</button>
        `;

        row.querySelector('.btn-remove-account').addEventListener('click', async () => {
          const removed = accountFromRow(row);
          if (removed.id) {
            try {
              await deleteAccount({ service, id: removed.id });
            } catch (e) {
              log(`削除失敗: ${serviceMeta[service].label} ${removed.name || removed.id} (${e.message || e})`, 'warn');
            }
          }
          row.remove();
          if (!list.querySelector('.account-row')) addAccountRow(service);
          queuePersistSetup();
        });

        row.querySelector('.account-name').addEventListener('input', queuePersistSetup);

        const tokenInput = row.querySelector('.account-token');
        tokenInput.addEventListener('focus', () => {
          if (row.dataset.tokenMasked === '1' && tokenInput.value === savedTokenMask) {
            setTimeout(() => tokenInput.select(), 0);
          }
        });
        tokenInput.addEventListener('input', () => {
          row.dataset.tokenMasked = '0';
          row.dataset.hasToken = tokenInput.value?.trim() ? '0' : row.dataset.hasToken;
          queuePersistSetup();
        });

        list.appendChild(row);
      }
    }

    function addAccountRow(service, account = null) {
      const existing = readAccountsFromDom(service);
      const next = account || defaultAccount(service, existing.length);
      writeAccountsToDom(service, [...existing, next]);
      queuePersistSetup();
    }

    function collectAccounts() {
      const collected = {};
      for (const service of Object.keys(serviceMeta)) {
        const rows = readAccountsFromDom(service);
        collected[service] = rows.map((acc, idx) => ({
          id: acc.id || makeAccountId(service),
          name: acc.name || `${serviceMeta[service].label} ${idx + 1}`,
          token: acc.token || '',
          hasToken: Boolean(acc.hasToken),
        }));
      }
      return collected;
    }

    return {
      makeAccountId,
      defaultAccount,
      accountFromRow,
      readAccountsFromDom,
      writeAccountsToDom,
      addAccountRow,
      collectAccounts,
      upsertDomTokenState,
    };
  }

  return {
    createAccountUi,
  };
}));
