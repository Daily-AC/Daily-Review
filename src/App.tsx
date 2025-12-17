import { useState, useEffect } from 'react';
import { invoke } from "@tauri-apps/api/core";
import ReactMarkdown from 'react-markdown';
import './App.css';

interface LogItem {
  id?: number;
  content: string;
  log_type: string;
  timestamp: string;
}

interface GitCommit {
  hash: string;
  message: string;
  author: string;
  time: number;
  repo_name?: string;
}

function App() {
  const [activeTab, setActiveTab] = useState<'log' | 'review' | 'settings'>('log');
  const [logs, setLogs] = useState<LogItem[]>([]);
  const [gitLogs, setGitLogs] = useState<GitCommit[]>([]);
  const [newLog, setNewLog] = useState('');
  
  // Settings State
  const [apiKey, setApiKey] = useState('');
  const [gitPaths, setGitPaths] = useState<string[]>([]);
  const [newGitPath, setNewGitPath] = useState('');
  const [apiProvider, setApiProvider] = useState('openai');
  const [baseUrl, setBaseUrl] = useState('');
  const [modelName, setModelName] = useState('gpt-4o');
  
  // Custom Rules
  const [customRules, setCustomRules] = useState('');
  const [reportTemplate, setReportTemplate] = useState(`Example:
1. Progress: ...
2. Problems: ...
3. Plan for tomorrow: ...`);

  // Review State
  const [reviewResult, setReviewResult] = useState('');
  const [reviewMode, setReviewMode] = useState<'analysis' | 'export'>('analysis');
  const [isAiLoading, setIsAiLoading] = useState(false);

  useEffect(() => {
    // Load Settings
    const savedKey = localStorage.getItem('apiKey');
    const savedPaths = localStorage.getItem('gitPaths'); 
    const savedProvider = localStorage.getItem('apiProvider');
    const savedBaseUrl = localStorage.getItem('baseUrl');
    const savedModel = localStorage.getItem('modelName');
    const savedRules = localStorage.getItem('customRules');
    const savedTemplate = localStorage.getItem('reportTemplate');
    
    if (savedKey) setApiKey(savedKey);
    if (savedPaths) {
        try { setGitPaths(JSON.parse(savedPaths)); } catch (e) { console.error('Failed', e); }
    }
    if (savedProvider) setApiProvider(savedProvider);
    if (savedBaseUrl) setBaseUrl(savedBaseUrl);
    if (savedModel) setModelName(savedModel);
    if (savedRules) setCustomRules(savedRules);
    if (savedTemplate) setReportTemplate(savedTemplate);

    loadLogs();
  }, []);

  const loadLogs = async () => {
    try {
      const todayLogs = await invoke<LogItem[]>('get_today_logs');
      setLogs(todayLogs);
    } catch (e) {
      console.error(e);
    }
  };

  const saveSettings = () => {
    localStorage.setItem('apiKey', apiKey);
    localStorage.setItem('gitPaths', JSON.stringify(gitPaths));
    localStorage.setItem('apiProvider', apiProvider);
    localStorage.setItem('baseUrl', baseUrl);
    localStorage.setItem('modelName', modelName);
    localStorage.setItem('customRules', customRules);
    localStorage.setItem('reportTemplate', reportTemplate);
    alert('Settings Saved Successfully!');
  };

  const addGitPath = () => {
      if(newGitPath && !gitPaths.includes(newGitPath)) {
          setGitPaths([...gitPaths, newGitPath]);
          setNewGitPath('');
      }
  };

  const removeGitPath = (pathToRemove: string) => {
      setGitPaths(gitPaths.filter(p => p !== pathToRemove));
  };

  const handleAddLog = async () => {
    if (!newLog.trim()) return;
    try {
      await invoke('save_log', { content: newLog, logType: 'manual' });
      setNewLog('');
      await loadLogs(); // Await the refresh
    } catch (e) {
      alert('Failed to save log: ' + e);
    }
  };
  
  const handleDeleteLog = async (id: number) => {
      if (!confirm('Delete this log?')) return;
      try {
          await invoke('delete_log', { id });
          await loadLogs(); // Await the refresh
      } catch (e) {
          alert('Failed to delete log: ' + e);
      }
  };

  const handleSyncGit = async () => {
    if (gitPaths.length === 0) {
      alert('Please configure Git Repo Paths in Settings first.');
      setActiveTab('settings');
      return;
    }
    try {
      const commits = await invoke<GitCommit[]>('scan_git_repos', { paths: gitPaths });
      setGitLogs(commits);
    } catch (e) {
      alert('Git Sync Failed: ' + e);
    }
  };

  const generatePrompt = (type: 'analysis' | 'export') => {
      const logsText = logs.map(l => `- ${l.content}`).join('\n');
      const gitText = gitLogs.map(g => `- [${g.repo_name}] ${g.message}`).join('\n');
      
      let baseInstruction = "";
      if (type === 'analysis') {
          baseInstruction = "Provide a comprehensive summary, 3 improvements, and 1 key knowledge point.";
      } else {
          baseInstruction = `Strictly follow the format below:\n\nFormat Template:\n${reportTemplate}`;
      }

      return `
        Context:
        Manual Logs:
        ${logsText}
        
        Git Commits:
        ${gitText}
        
        System Instruction:
        ${baseInstruction}
        
        Additional User Rules:
        ${customRules}
      `;
  };

  const handleAiReview = async (mode: 'analysis' | 'export') => {
    if (!apiKey) {
      alert('Please configure API Key in Settings first.');
      setActiveTab('settings');
      return;
    }
    setReviewMode(mode);
    setIsAiLoading(true);
    try {
      const prompt = generatePrompt(mode);
      const response = await invoke<string>('call_ai', { 
        request: {
          provider: apiProvider,
          api_key: apiKey,
          model: modelName,
          prompt: prompt,
          base_url: baseUrl || null 
        }
      });
      setReviewResult(response);
    } catch (e) {
      alert('AI Action Failed: ' + e);
    } finally {
      setIsAiLoading(false);
    }
  };

  const copyToClipboard = () => {
      navigator.clipboard.writeText(reviewResult);
      alert('Copied to clipboard!');
  };

  return (
    <div className="app-container animate-fade-in">
      <header>
        <div>
          <h1 style={{ 
            fontSize: '1.8rem', 
            background: 'linear-gradient(to right, #818cf8, #22d3ee)', 
            WebkitBackgroundClip: 'text', 
            WebkitTextFillColor: 'transparent',
          }}>
            Daily Flow
          </h1>
        </div>
        
        <nav className="tab-nav">
          <button className={`tab-btn ${activeTab === 'log' ? 'active' : ''}`} onClick={() => setActiveTab('log')}>Today</button>
          <button className={`tab-btn ${activeTab === 'review' ? 'active' : ''}`} onClick={() => setActiveTab('review')}>Review</button>
          <button className={`tab-btn ${activeTab === 'settings' ? 'active' : ''}`} onClick={() => setActiveTab('settings')}>settings</button>
        </nav>
      </header>

      <main className="content-area glass-panel">
        {activeTab === 'log' && (
          <div className="animate-fade-in">
            <div style={{ display: 'flex', gap: '12px', marginBottom: '24px' }}>
              <input 
                value={newLog}
                autoFocus
                onChange={(e) => setNewLog(e.target.value)}
                placeholder="What are you working on?" 
                onKeyDown={(e) => e.key === 'Enter' && handleAddLog()}
              />
              <button className="glass-button btn-primary" onClick={handleAddLog}>Add Note</button>
              <button className="glass-button" onClick={handleSyncGit}>Sync Git</button>
            </div>
            
            <div className="space-y-4">
               {logs.map(log => (
                 <div key={log.id} className="log-item" style={{justifyContent: 'space-between'}}>
                    <div style={{display: 'flex', gap: '12px', alignItems: 'flex-start'}}>
                        <span className="timestamp-pill">{log.timestamp.split(' ')[1].substring(0,5)}</span>
                        <span style={{ color: '#e2e8f0' }}>{log.content}</span>
                    </div>
                    <button 
                        onClick={() => handleDeleteLog(log.id!)}
                        className="delete-btn"
                        style={{
                            background: 'transparent',
                            border: 'none',
                            color: '#94a3b8',
                            cursor: 'pointer',
                            padding: '4px',
                            opacity: 0.6,
                            transition: 'all 0.2s',
                            fontSize: '0.9em'
                        }}
                        onMouseOver={(e) => e.currentTarget.style.color = '#ef4444'}
                        onMouseOut={(e) => e.currentTarget.style.color = '#94a3b8'}
                    >
                        🗑️
                    </button>
                 </div>
               ))}
               
               {gitLogs.length > 0 && (
                   <div style={{ marginTop: '32px', marginBottom: '16px' }}>
                       <h3 className="text-secondary text-sm" style={{ marginBottom: '12px' }}>GIT ACTIVITY ({gitLogs.length})</h3>
                       {gitLogs.map((commit, idx) => (
                         <div key={idx} className="log-item" style={{ borderLeft: '3px solid #22d3ee' }}>
                            <span className="timestamp-pill" style={{ background: 'rgba(34, 211, 238, 0.1)', color: '#22d3ee' }}>
                                {commit.repo_name}
                            </span>
                            <span style={{ color: '#cbd5e1', fontSize: '0.95em' }}>{commit.message}</span>
                         </div>
                       ))}
                   </div>
               )}
            </div>
          </div>
        )}

        {activeTab === 'review' && (
          <div className="animate-fade-in" style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
             <div style={{ marginBottom: '24px', display: 'flex', gap: '16px', justifyContent: 'center' }}>
                <button 
                  className={`glass-button ${reviewMode === 'analysis' ? 'btn-primary' : ''}`}
                  onClick={() => handleAiReview('analysis')}
                  disabled={isAiLoading}
                  style={{ minWidth: '160px' }}
                >
                   {isAiLoading && reviewMode === 'analysis' ? 'Thinking...' : 'Start Analysis'}
                </button>
                <button 
                  className={`glass-button ${reviewMode === 'export' ? 'btn-primary' : ''}`}
                  onClick={() => handleAiReview('export')}
                  disabled={isAiLoading}
                  style={{ minWidth: '160px', background: reviewMode === 'export' ? '' : 'transparent' }}
                >
                   {isAiLoading && reviewMode === 'export' ? 'Generating...' : 'Export Report'}
                </button>
             </div>

             {reviewResult && (
               <div className="animate-fade-in" style={{ position: 'relative', flex: 1, overflow: 'hidden' }}>
                  <button 
                    onClick={copyToClipboard}
                    className="glass-button"
                    style={{ position: 'absolute', right: '16px', top: '16px', zIndex: 10, padding: '6px 12px', fontSize: '0.8rem' }}
                  >
                    Copy Text
                  </button>
                  <div style={{ 
                      height: '100%', 
                      overflowY: 'auto', 
                      background: 'rgba(0,0,0,0.2)', 
                      borderRadius: '12px', 
                      padding: '24px' 
                    }} className="markdown-content">
                     <ReactMarkdown>{reviewResult}</ReactMarkdown>
                  </div>
               </div>
             )}
              
             {!reviewResult && !isAiLoading && (
                 <div style={{ textAlign: 'center', marginTop: '100px', color: 'var(--text-secondary)' }}>
                     <p>Ready to analyze {logs.length} logs and {gitLogs.length} commits.</p>
                 </div>
             )}
          </div>
        )}

        {activeTab === 'settings' && (
           <div className="animate-fade-in grid-2" style={{ maxWidth: '900px', margin: '0 auto' }}>
                {/* Left Column: AI Configuration */}
                <div>
                   <h3 style={{ marginBottom: '20px', color: 'var(--accent-primary)' }}>AI Configuration</h3>
                   
                   <div className="mb-4">
                     <label>AI Provider</label>
                     <select value={apiProvider} onChange={(e) => setApiProvider(e.target.value)}>
                       <option value="openai">OpenAI / Compatible</option>
                       <option value="anthropic">Anthropic</option>
                       <option value="gemini">Gemini</option>
                     </select>
                   </div>
                   
                   <div className="mb-4">
                     <label>Base URL <span className="text-secondary" style={{opacity:0.5}}>(Optional)</span></label>
                     <input 
                        value={baseUrl} 
                        onChange={(e) => setBaseUrl(e.target.value)} 
                        placeholder="https://api.example.com/v1" 
                     />
                   </div>
                   
                   <div className="mb-4">
                     <label>Model Name</label>
                     <input 
                        value={modelName} 
                        onChange={(e) => setModelName(e.target.value)} 
                        placeholder="gpt-4o" 
                     />
                   </div>
                   
                   <div className="mb-4">
                     <label>API Key</label>
                     <input 
                        type="password" 
                        value={apiKey} 
                        onChange={(e) => setApiKey(e.target.value)} 
                        placeholder="sk-..." 
                     />
                   </div>

                   <button className="glass-button btn-primary" style={{ width: '100%', marginTop: '20px' }} onClick={saveSettings}>
                     Save Configuration
                   </button>
                </div>

                {/* Right Column: Data & Rules */}
                <div>
                  <h3 style={{ marginBottom: '20px', color: 'var(--accent-secondary)' }}>Data Source & Rules</h3>
                  
                  <div className="mb-4">
                    <label>Git Repositories</label>
                    <div style={{ display: 'flex', gap: '8px', marginBottom: '12px' }}>
                        <input 
                            value={newGitPath} 
                            onChange={(e) => setNewGitPath(e.target.value)} 
                            placeholder="C:/Projects/MyRepo" 
                        />
                        <button onClick={addGitPath} className="glass-button" style={{ width: 'auto' }}>+</button>
                    </div>
                    <div style={{ 
                        maxHeight: '120px', 
                        overflowY: 'auto', 
                        background: 'rgba(0,0,0,0.2)', 
                        borderRadius: '8px',
                        padding: '8px' 
                    }}>
                        {gitPaths.map(p => (
                            <div key={p} style={{ display: 'flex', justifyContent: 'space-between', padding: '6px', borderBottom: '1px solid rgba(255,255,255,0.05)' }}>
                                <span style={{ fontSize: '0.85rem', color: '#cbd5e1' }} className="truncate">{p}</span>
                                <button onClick={() => removeGitPath(p)} style={{ background: 'none', border: 'none', color: '#ef4444', cursor: 'pointer' }}>×</button>
                            </div>
                        ))}
                        {gitPaths.length === 0 && <span style={{fontSize: '0.8rem', color: '#64748b'}}>No repositories added yet.</span>}
                    </div>
                  </div>

                  <div className="mb-4">
                     <label>Custom Rules</label>
                     <textarea 
                        value={customRules} 
                        onChange={(e) => setCustomRules(e.target.value)}
                        placeholder="e.g. Always include a quote..."
                        style={{ height: '80px', resize: 'none' }}
                     />
                  </div>
                  
                  <div className="mb-4">
                     <label>Export Template</label>
                     <textarea 
                        value={reportTemplate} 
                        onChange={(e) => setReportTemplate(e.target.value)}
                        style={{ height: '80px', resize: 'none' }}
                     />
                  </div>
                </div>
           </div>
        )}
      </main>
    </div>
  );
}

export default App;
