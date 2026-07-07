const { invoke } = window.__TAURI__.core;
const { open } = window.__TAURI__.dialog;

// Global state
let currentTab = 'status';
let isServerRunning = false;
let sharedRootPath = null;
let currentPcPath = '/';
let localServerIpPort = '127.0.0.1:8080';

// Core UI setup
document.addEventListener('DOMContentLoaded', () => {
    // Tab switching
    const navLinks = document.querySelectorAll('.nav-links li');
    navLinks.forEach(link => {
        link.addEventListener('click', () => {
            const tabId = link.getAttribute('data-tab');
            switchTab(tabId);
        });
    });

    // Setup Status Tab
    setupStatusTab();
});

function switchTab(tabId) {
    document.querySelectorAll('.nav-links li').forEach(l => l.classList.remove('active'));
    document.querySelector(`.nav-links li[data-tab="${tabId}"]`).classList.add('active');

    document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
    document.getElementById(tabId).classList.add('active');
    
    currentTab = tabId;
}

// Status Tab Logic
function setupStatusTab() {
    const toggleBtn = document.getElementById('toggle-server-btn');
    const detailsDiv = document.getElementById('server-details');
    
    toggleBtn.addEventListener('click', async () => {
        if (isServerRunning) {
            try {
                await invoke('stop_local_server');
                isServerRunning = false;
                toggleBtn.textContent = 'Start Server';
                toggleBtn.classList.remove('danger');
                toggleBtn.classList.add('primary');
                detailsDiv.classList.add('hidden');
            } catch (e) {
                console.error(e);
                alert("Failed to stop server: " + e);
            }
        } else {
            if (!sharedRootPath) {
                // Prompt to select folder first
                const selected = await open({ directory: true, multiple: false });
                if (selected) {
                    sharedRootPath = selected;
                    document.getElementById('current-root-path').textContent = sharedRootPath;
                    document.getElementById('pc-actions').classList.remove('hidden');
                    // We'll load PC files later when the My PC tab logic is implemented
                } else {
                    return; // Cancelled
                }
            }
            
            try {
                toggleBtn.textContent = 'Starting...';
                toggleBtn.disabled = true;
                
                const status = await invoke('start_local_server', { rootPathStr: sharedRootPath });
                
                isServerRunning = true;
                localServerIpPort = `${status.ip}:${status.port}`;
                toggleBtn.textContent = 'Stop Server';
                toggleBtn.classList.remove('primary');
                toggleBtn.classList.add('danger');
                toggleBtn.disabled = false;
                
                document.getElementById('server-address').textContent = `http://${status.ip}:${status.port}/`;
                document.getElementById('server-username').textContent = status.username;
                document.getElementById('server-password').textContent = status.password;
                
                detailsDiv.classList.remove('hidden');
            } catch (e) {
                console.error(e);
                alert("Failed to start server: " + e);
                toggleBtn.textContent = 'Start Server';
                toggleBtn.disabled = false;
            }
        }
    });
}

// My PC Tab Logic
let selectedPcFiles = new Set();

document.addEventListener('DOMContentLoaded', () => {
    setupMyPcTab();
});

function setupMyPcTab() {
    const selectRootBtn = document.getElementById('select-root-btn');
    const refreshBtn = document.getElementById('pc-refresh-btn');
    const mkdirBtn = document.getElementById('pc-mkdir-btn');
    const deleteBtn = document.getElementById('pc-delete-btn');
    const zipBtn = document.getElementById('pc-zip-btn');
    
    selectRootBtn.addEventListener('click', async () => {
        const selected = await open({ directory: true, multiple: false });
        if (selected) {
            sharedRootPath = selected;
            document.getElementById('current-root-path').textContent = sharedRootPath;
            document.getElementById('pc-actions').classList.remove('hidden');
            currentPcPath = '/';
            
            // If server is running, we should restart it or just load files
            // For simplicity, we just fetch files natively via a command or locally via fetch
            // But since our server might not be running on loopback, we can just fetch via localhost:8080 if running.
            // Wait, the easiest way to browse local PC is via a Tauri command since we already wrote `list_files` in rest.rs.
            // But we don't have a direct Tauri command for `list_files`. We have an HTTP endpoint!
            // Let's just fetch from the local HTTP endpoint if the server is running.
            // If it's not running, we must tell the user to start the server.
            if (!isServerRunning) {
                renderPcEmpty("Server must be running to browse files.");
            } else {
                loadPcFiles(currentPcPath);
            }
        }
    });

    refreshBtn.addEventListener('click', () => {
        if (isServerRunning) loadPcFiles(currentPcPath);
    });

    mkdirBtn.addEventListener('click', async () => {
        const name = prompt("Enter new folder name:");
        if (name && isServerRunning) {
            const targetPath = currentPcPath === '/' ? `/${name}` : `${currentPcPath}/${name}`;
            await authFetch(`http://${localServerIpPort}/api/mkdir?path=${encodeURIComponent(targetPath)}`, { method: 'POST' });
            loadPcFiles(currentPcPath);
        }
    });

    deleteBtn.addEventListener('click', async () => {
        if (selectedPcFiles.size === 0) return;
        if (!confirm(`Delete ${selectedPcFiles.size} item(s)?`)) return;
        
        for (const path of selectedPcFiles) {
            await authFetch(`http://${localServerIpPort}/api/delete?path=${encodeURIComponent(path)}`, { method: 'POST' });
        }
        selectedPcFiles.clear();
        loadPcFiles(currentPcPath);
    });

    zipBtn.addEventListener('click', () => {
        if (selectedPcFiles.size === 0) {
            alert("Select files/folders to zip download");
            return;
        }
        const paths = Array.from(selectedPcFiles).join(',');
        const url = `http://${localServerIpPort}/api/zip?paths=${encodeURIComponent(paths)}`;
        
        // Use browser download
        const a = document.createElement('a');
        a.href = url;
        // We need auth though. Native <a> tag doesn't send Basic Auth unless embedded in URL
        const username = document.getElementById('server-username').textContent;
        const password = document.getElementById('server-password').textContent;
        a.href = `http://${username}:${password}@${localServerIpPort}/api/zip?paths=${encodeURIComponent(paths)}`;
        a.download = "download.zip";
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
    });
}

function renderPcEmpty(msg) {
    const list = document.getElementById('pc-file-list');
    list.innerHTML = `<li class="empty-state">${msg}</li>`;
}

async function authFetch(url, options = {}) {
    const username = document.getElementById('server-username').textContent;
    const password = document.getElementById('server-password').textContent;
    
    if (!options.headers) options.headers = {};
    options.headers['Authorization'] = 'Basic ' + btoa(username + ':' + password);
    
    const res = await fetch(url, options);
    if (!res.ok) throw new Error(res.statusText);
    return res;
}

async function loadPcFiles(path) {
    try {
        const res = await authFetch(`http://${localServerIpPort}/api/list?path=${encodeURIComponent(path)}`);
        const files = await res.json();
        
        selectedPcFiles.clear();
        currentPcPath = path;
        
        // Update breadcrumb
        const breadcrumb = document.getElementById('pc-breadcrumb');
        breadcrumb.innerHTML = '';
        
        const parts = path.split('/').filter(p => p);
        let accumulated = '';
        
        const rootSpan = document.createElement('span');
        rootSpan.textContent = '/';
        rootSpan.onclick = () => loadPcFiles('/');
        breadcrumb.appendChild(rootSpan);
        
        for (const part of parts) {
            accumulated += '/' + part;
            const span = document.createElement('span');
            span.textContent = part + '/';
            let target = accumulated;
            span.onclick = () => loadPcFiles(target);
            breadcrumb.appendChild(span);
        }

        const list = document.getElementById('pc-file-list');
        list.innerHTML = '';
        
        if (path !== '/') {
            const upLi = document.createElement('li');
            upLi.innerHTML = `<span class="file-icon">📁</span><span class="file-name">..</span>`;
            upLi.onclick = () => {
                let upPath = path.substring(0, path.lastIndexOf('/'));
                if (upPath === '') upPath = '/';
                loadPcFiles(upPath);
            };
            list.appendChild(upLi);
        }

        if (files.length === 0) {
            list.innerHTML += `<li class="empty-state">Empty folder</li>`;
            return;
        }

        files.sort((a, b) => {
            if (a.is_dir && !b.is_dir) return -1;
            if (!a.is_dir && b.is_dir) return 1;
            return a.name.localeCompare(b.name);
        });

        files.forEach(f => {
            const li = document.createElement('li');
            li.innerHTML = `
                <span class="file-icon">${f.is_dir ? '📁' : '📄'}</span>
                <span class="file-name">${f.name}</span>
                <span class="file-size">${f.is_dir ? '' : formatBytes(f.size)}</span>
            `;
            
            li.onclick = (e) => {
                if (e.ctrlKey || e.metaKey) {
                    // Multi-select
                    if (selectedPcFiles.has(f.path)) {
                        selectedPcFiles.delete(f.path);
                        li.classList.remove('selected');
                    } else {
                        selectedPcFiles.add(f.path);
                        li.classList.add('selected');
                    }
                } else {
                    if (f.is_dir) {
                        loadPcFiles(f.path);
                    } else {
                        // Single select
                        document.querySelectorAll('#pc-file-list li').forEach(el => el.classList.remove('selected'));
                        selectedPcFiles.clear();
                        selectedPcFiles.add(f.path);
                        li.classList.add('selected');
                    }
                }
            };
            list.appendChild(li);
        });

    } catch (e) {
        renderPcEmpty("Error loading files: " + e.message);
    }
}

function formatBytes(bytes) {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

// Hook into status tab start button to load PC files automatically
const oldToggle = document.getElementById('toggle-server-btn').onclick;
// we used addEventListener, so we can't just override onclick.
// Let's add another listener
document.getElementById('toggle-server-btn').addEventListener('click', () => {
    setTimeout(() => {
        if (isServerRunning && sharedRootPath) {
            loadPcFiles('/');
        } else if (!isServerRunning) {
            renderPcEmpty("Server must be running to browse files.");
        }
    }, 500); // give server time to start
});

// Phone Files Tab Logic
let phoneConnected = false;
let phoneBaseUrl = '';
let phoneAuthHeader = '';
let currentPhonePath = '/';
let selectedPhoneFiles = new Set();

document.addEventListener('DOMContentLoaded', () => {
    setupPhoneTab();
});

function setupPhoneTab() {
    const autoDiscoverBtn = document.getElementById('auto-discover-btn');
    const manualConnectBtn = document.getElementById('manual-connect-btn');
    const disconnectBtn = document.getElementById('disconnect-btn');
    const statusSpan = document.getElementById('discover-status');
    
    autoDiscoverBtn.addEventListener('click', async () => {
        statusSpan.textContent = "Scanning network...";
        autoDiscoverBtn.disabled = true;
        try {
            const ip = await invoke('discover_phone_cmd');
            document.getElementById('manual-ip').value = ip;
            statusSpan.textContent = `Found phone at ${ip}`;
        } catch (e) {
            statusSpan.textContent = "Error: " + e;
        } finally {
            autoDiscoverBtn.disabled = false;
        }
    });

    manualConnectBtn.addEventListener('click', async () => {
        const ip = document.getElementById('manual-ip').value.trim();
        const pass = document.getElementById('manual-pass').value.trim();
        
        if (!ip || !pass) {
            alert("Please enter IP and password");
            return;
        }

        // Clean IP if they pasted full url
        let cleanIp = ip.replace('http://', '').replace('/', '');
        if (cleanIp.includes(':')) cleanIp = cleanIp.split(':')[0]; // remove port

        phoneBaseUrl = `http://${cleanIp}:8080`;
        phoneAuthHeader = 'Basic ' + btoa(`usbshare:${pass}`);

        try {
            // Test connection
            await phoneFetch(`/api/list?path=/`);
            
            phoneConnected = true;
            document.getElementById('phone-connection-card').classList.add('hidden');
            document.getElementById('phone-browser-container').classList.remove('hidden');
            loadPhoneFiles('/');
        } catch (e) {
            alert("Failed to connect to phone: " + e.message);
        }
    });

    disconnectBtn.addEventListener('click', () => {
        phoneConnected = false;
        phoneBaseUrl = '';
        phoneAuthHeader = '';
        document.getElementById('phone-connection-card').classList.remove('hidden');
        document.getElementById('phone-browser-container').classList.add('hidden');
    });

    // Browser actions
    document.getElementById('phone-refresh-btn').addEventListener('click', () => loadPhoneFiles(currentPhonePath));
    
    document.getElementById('phone-mkdir-btn').addEventListener('click', async () => {
        const name = prompt("Enter new folder name:");
        if (name && phoneConnected) {
            const targetPath = currentPhonePath === '/' ? `/${name}` : `${currentPhonePath}/${name}`;
            await phoneFetch(`/api/mkdir?path=${encodeURIComponent(targetPath)}`, { method: 'POST' });
            loadPhoneFiles(currentPhonePath);
        }
    });

    document.getElementById('phone-delete-btn').addEventListener('click', async () => {
        if (selectedPhoneFiles.size === 0) return;
        if (!confirm(`Delete ${selectedPhoneFiles.size} item(s) from phone?`)) return;
        
        for (const path of selectedPhoneFiles) {
            await phoneFetch(`/api/delete?path=${encodeURIComponent(path)}`, { method: 'POST' });
        }
        selectedPhoneFiles.clear();
        loadPhoneFiles(currentPhonePath);
    });

    document.getElementById('phone-download-btn').addEventListener('click', () => {
        if (selectedPhoneFiles.size === 0) {
            alert("Select files/folders to download");
            return;
        }
        const paths = Array.from(selectedPhoneFiles).join(',');
        const a = document.createElement('a');
        
        const credentials = atob(phoneAuthHeader.split(' ')[1]);
        const ipPort = phoneBaseUrl.replace('http://', '');
        
        a.href = `http://${credentials}@${ipPort}/api/zip?paths=${encodeURIComponent(paths)}`;
        a.download = "phone_download.zip";
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
    });
    
    document.getElementById('phone-upload-btn').addEventListener('click', async () => {
        const selected = await open({ multiple: true, directory: false });
        if (!selected || selected.length === 0) return;
        
        // Use Tauri's read function and upload via fetch
        const { readFile } = window.__TAURI__.fs;
        const { basename } = window.__TAURI__.path;
        
        const progContainer = document.getElementById('phone-progress-container');
        const progFill = document.getElementById('phone-progress-fill');
        const progText = document.getElementById('phone-progress-text');
        
        progContainer.classList.remove('hidden');
        
        for (let i = 0; i < selected.length; i++) {
            const filePath = selected[i];
            const fileName = await basename(filePath);
            progText.textContent = `Uploading ${fileName} (${i+1}/${selected.length})...`;
            
            try {
                const contents = await readFile(filePath);
                const targetPath = currentPhonePath === '/' ? `/${fileName}` : `${currentPhonePath}/${fileName}`;
                
                await phoneFetch(`/api/upload?path=${encodeURIComponent(targetPath)}`, {
                    method: 'POST',
                    body: contents,
                    headers: { 'Content-Type': 'application/octet-stream' }
                });
                
                progFill.style.width = `${((i + 1) / selected.length) * 100}%`;
            } catch (e) {
                console.error("Upload error", e);
                alert(`Failed to upload ${fileName}: ` + e);
            }
        }
        
        setTimeout(() => {
            progContainer.classList.add('hidden');
            progFill.style.width = '0%';
            loadPhoneFiles(currentPhonePath);
        }, 1000);
    });
}

async function phoneFetch(endpoint, options = {}) {
    const url = phoneBaseUrl + endpoint;
    if (!options.headers) options.headers = {};
    options.headers['Authorization'] = phoneAuthHeader;
    
    const res = await fetch(url, options);
    if (!res.ok) throw new Error(res.statusText);
    return res;
}

async function loadPhoneFiles(path) {
    try {
        const res = await phoneFetch(`/api/list?path=${encodeURIComponent(path)}`);
        const files = await res.json();
        
        selectedPhoneFiles.clear();
        currentPhonePath = path;
        
        // Update breadcrumb
        const breadcrumb = document.getElementById('phone-breadcrumb');
        breadcrumb.innerHTML = '';
        
        const parts = path.split('/').filter(p => p);
        let accumulated = '';
        
        const rootSpan = document.createElement('span');
        rootSpan.textContent = '/';
        rootSpan.onclick = () => loadPhoneFiles('/');
        breadcrumb.appendChild(rootSpan);
        
        for (const part of parts) {
            accumulated += '/' + part;
            const span = document.createElement('span');
            span.textContent = part + '/';
            let target = accumulated;
            span.onclick = () => loadPhoneFiles(target);
            breadcrumb.appendChild(span);
        }

        const list = document.getElementById('phone-file-list');
        list.innerHTML = '';
        
        if (path !== '/') {
            const upLi = document.createElement('li');
            upLi.innerHTML = `<span class="file-icon">📁</span><span class="file-name">..</span>`;
            upLi.onclick = () => {
                let upPath = path.substring(0, path.lastIndexOf('/'));
                if (upPath === '') upPath = '/';
                loadPhoneFiles(upPath);
            };
            list.appendChild(upLi);
        }

        if (files.length === 0) {
            list.innerHTML += `<li class="empty-state">Empty folder</li>`;
            return;
        }

        files.sort((a, b) => {
            if (a.is_dir && !b.is_dir) return -1;
            if (!a.is_dir && b.is_dir) return 1;
            return a.name.localeCompare(b.name);
        });

        files.forEach(f => {
            const li = document.createElement('li');
            li.innerHTML = `
                <span class="file-icon">${f.is_dir ? '📁' : '📄'}</span>
                <span class="file-name">${f.name}</span>
                <span class="file-size">${f.is_dir ? '' : formatBytes(f.size)}</span>
            `;
            
            li.onclick = (e) => {
                if (e.ctrlKey || e.metaKey) {
                    if (selectedPhoneFiles.has(f.path)) {
                        selectedPhoneFiles.delete(f.path);
                        li.classList.remove('selected');
                    } else {
                        selectedPhoneFiles.add(f.path);
                        li.classList.add('selected');
                    }
                } else {
                    if (f.is_dir) {
                        loadPhoneFiles(f.path);
                    } else {
                        document.querySelectorAll('#phone-file-list li').forEach(el => el.classList.remove('selected'));
                        selectedPhoneFiles.clear();
                        selectedPhoneFiles.add(f.path);
                        li.classList.add('selected');
                    }
                }
            };
            list.appendChild(li);
        });

    } catch (e) {
        document.getElementById('phone-file-list').innerHTML = `<li class="empty-state">Error loading files: ${e.message}</li>`;
    }
}
