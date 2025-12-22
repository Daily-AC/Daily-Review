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
  diff?: string;
}

// Backend Config Interface
interface AppConfig {
    api_key: string;
    git_paths: string[];
    provider: string;
    model: string;
    base_url: string | null;
    custom_rules: string;
    report_template: string;
    deep_analysis: boolean;
}

function App() {
  const [activeTab, setActiveTab] = useState<'log' | 'review' | 'settings'>('log');
  const [logs, setLogs] = useState<LogItem[]>([]);
  const [gitLogs, setGitLogs] = useState<GitCommit[]>([]);
  const [newLog, setNewLog] = useState('');
  
  // Settings State
  const [config, setConfig] = useState<AppConfig>({
      api_key: '',
      git_paths: [],
      provider: 'openai',
      model: 'gpt-4o',
      base_url: null,
      custom_rules: '',
      report_template: '',
      deep_analysis: false
  });

  const [newGitPath, setNewGitPath] = useState('');
  
  // Review State
  const [reviewResult, setReviewResult] = useState('');
  const [reviewMode, setReviewMode] = useState<'analysis' | 'export'>('analysis');
  const [isAiLoading, setIsAiLoading] = useState(false);

  useEffect(() => {
    // Load Config from Backend
    const init = async () => {
        try {
            const loadedConfig = await invoke<AppConfig>('get_config');
            setConfig(loadedConfig);
        } catch (e) {
            console.error('Failed to load config', e);
        }
    };
    init();
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

  const saveSettings = async () => {
    try {
        await invoke('save_config', { config });
        alert('Settings Saved Successfully to Backend!');
    } catch (e) {
        alert('Failed to save settings: ' + e);
    }
  };

  const updateConfig = (key: keyof AppConfig, value: any) => {
      setConfig(prev => ({ ...prev, [key]: value }));
  };

  const addGitPath = () => {
      if(newGitPath && !config.git_paths.includes(newGitPath)) {
          updateConfig('git_paths', [...config.git_paths, newGitPath]);
          setNewGitPath('');
      }
  };

  const removeGitPath = (pathToRemove: string) => {
      updateConfig('git_paths', config.git_paths.filter(p => p !== pathToRemove));
  };

  const handleAddLog = async () => {
    if (!newLog.trim()) return;
    try {
      await invoke('save_log', { content: newLog, logType: 'manual' });
      setNewLog('');
      await loadLogs(); 
    } catch (e) {
      alert('Failed to save log: ' + e);
    }
  };
  
  const handleDeleteLog = async (id: number) => {
      if (!confirm('Delete this log?')) return;
      try {
          await invoke('delete_log', { id });
          await loadLogs(); 
      } catch (e) {
          alert('Failed to delete log: ' + e);
      }
  };

  const handleSyncGit = async () => {
    if (config.git_paths.length === 0) {
      alert('Please configure Git Repo Paths in Settings first.');
      setActiveTab('settings');
      return;
    }
    try {
      const commits = await invoke<GitCommit[]>('scan_git_repos', { 
          paths: config.git_paths, 
          deepAnalysis: config.deep_analysis 
      });
      setGitLogs(commits);
      if (config.deep_analysis) {
          alert(`Synced ${commits.length} commits with deep analysis.`);
      }
    } catch (e) {
      alert('Git Sync Failed: ' + e);
    }
  };

  // Deprecated on Frontend, but kept for immediate preview or fallbacks
  // Ideally backend generates prompt now, but for frontend 'generatePrompt' we can keep or remove.
  // We'll keep a simple version or just rely on backend info if logic is fully moved.
  // Actually, for immediate GUI feedback, the frontend might still constructing the prompt text to send?
  // Wait, the backend command `call_ai` expects `prompt` string in `AiRequest`.
  // The Backend Logic `generate_prompt_logic` is internal helper.
  // Let's check `lib.rs`: `call_ai` takes `AiRequest` which has `prompt`.
  // The CLI `Review` command calls `generate_prompt_logic` then `call_ai`.
  // The GUI calls `call_ai`. So GUI MUST still generate the prompt OR we need a new Tauri command `generate_review` that internally does it all.
  // Checking `lib.rs` again... `Commands::Review` does the internal logic.
  // `call_ai` is raw.
  // To keep it clean, I should probably expose a `ai_review_workflow` command or similar?
  // OR just update frontend to duplicate logic (current state) OR `invoke('generate_prompt', ...)`?
  // `lib.rs` does NOT expose `generate_prompt` as a tauri command.
  // However, I can still generate prompt in frontend using the config state.
  // I will maintain frontend prompt gen for now to minimize risk, utilizing the new config state.

  const generatePrompt = (type: 'analysis' | 'export') => {
      const logsText = logs.map(l => `- ${l.content}`).join('\n');
      const gitText = gitLogs.map(g => {
          let text = `- [${g.repo_name}] ${g.message}`;
          if (g.diff) {
              text += `\n  Code Diff Summary:\n\`\`\`\n${g.diff}\n\`\`\``;
          }
          return text;
      }).join('\n');
      
      let baseInstruction = "";
      if (type === 'analysis') {
          baseInstruction = "Provide a comprehensive summary, 3 improvements, and 1 key knowledge point. If code diffs are provided, use them to explain technical details.";
      } else {
          baseInstruction = `Strictly follow the format below:\n\nFormat Template:\n${config.report_template}`;
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
        ${config.custom_rules}
      `;
  };

  const handleAiReview = async (mode: 'analysis' | 'export') => {
    if (!config.api_key) {
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
          provider: config.provider,
          api_key: config.api_key,
          model: config.model,
          prompt: prompt,
          base_url: config.base_url 
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
          <button className={`tab-btn ${activeTab === 'settings' ? 'active' : ''}`} onClick={() => setActiveTab('settings')}>Settings</button>
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
              <button className="glass-button" onClick={handleSyncGit}>
                  {config.deep_analysis ? 'Sync Deep' : 'Sync Git'}
              </button>
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
                       <div style={{display:'flex', justifyContent:'space-between', alignItems:'center', marginBottom: '12px'}}>
                            <h3 className="text-secondary text-sm">GIT ACTIVITY ({gitLogs.length})</h3>
                            {config.deep_analysis && <span style={{fontSize:'0.7rem', color:'#22d3ee', border:'1px solid #22d3ee', padding:'2px 6px', borderRadius:'4px'}}>Deep Analysis ON</span>}
                       </div>
                       {gitLogs.map((commit, idx) => (
                         <div key={idx} className="log-item" style={{ borderLeft: '3px solid #22d3ee' }}>
                            <span className="timestamp-pill" style={{ background: 'rgba(34, 211, 238, 0.1)', color: '#22d3ee' }}>
                                {commit.repo_name}
                            </span>
                            <div style={{ flex: 1 }}>
                                <div style={{ color: '#cbd5e1', fontSize: '0.95em' }}>{commit.message}</div>
                                {commit.diff && (
                                    <div style={{ 
                                        marginTop: '8px', 
                                        fontSize: '0.8rem', 
                                        fontFamily: 'monospace', 
                                        color: '#94a3b8',
                                        maxHeight: '60px',
                                        overflow: 'hidden',
                                        position: 'relative'
                                    }}>
                                        {commit.diff.substring(0, 150)}...
                                    </div>
                                )}
                            </div>
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
                     {config.deep_analysis && <p style={{color: '#22d3ee', fontSize: '0.9rem', marginTop:'8px'}}>Deep Analysis Enabled</p>}
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
                     <select value={config.provider} onChange={(e) => updateConfig('provider', e.target.value)}>
                       <option value="openai">OpenAI / Compatible</option>
                       <option value="anthropic">Anthropic</option>
                       <option value="gemini">Gemini</option>
                     </select>
                   </div>
                   
                   <div className="mb-4">
                     <label>Base URL <span className="text-secondary" style={{opacity:0.5}}>(Optional)</span></label>
                     <input 
                        value={config.base_url || ''} 
                        onChange={(e) => updateConfig('base_url', e.target.value)} 
                        placeholder="https://api.example.com/v1" 
                     />
                   </div>
                   
                   <div className="mb-4">
                     <label>Model Name</label>
                     <input 
                        value={config.model} 
                        onChange={(e) => updateConfig('model', e.target.value)} 
                        placeholder="gpt-4o" 
                     />
                   </div>
                   
                   <div className="mb-4">
                     <label>API Key</label>
                     <input 
                        type="password" 
                        value={config.api_key} 
                        onChange={(e) => updateConfig('api_key', e.target.value)} 
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
                        {config.git_paths.map(p => (
                            <div key={p} style={{ display: 'flex', justifyContent: 'space-between', padding: '6px', borderBottom: '1px solid rgba(255,255,255,0.05)' }}>
                                <span style={{ fontSize: '0.85rem', color: '#cbd5e1' }} className="truncate">{p}</span>
                                <button onClick={() => removeGitPath(p)} style={{ background: 'none', border: 'none', color: '#ef4444', cursor: 'pointer' }}>×</button>
                            </div>
                        ))}
                        {config.git_paths.length === 0 && <span style={{fontSize: '0.8rem', color: '#64748b'}}>No repositories added yet.</span>}
                    </div>
                  </div>

                  {/* Deep Analysis Toggle */}
                   <div className="mb-4" style={{ 
                       background: 'rgba(34, 211, 238, 0.05)', 
                       padding: '12px', 
                       borderRadius: '8px', 
                       border: '1px solid rgba(34, 211, 238, 0.1)',
                       display: 'flex',
                       alignItems: 'center',
                       justifyContent: 'space-between'
                   }}>
                     <div>
                       <label style={{marginBottom: '4px', display:'block', color:'#e2e8f0'}}>Deep Git Analysis</label>
                       <span style={{fontSize: '0.75rem', color: '#94a3b8'}}>Analyze code diffs for better insights. (Slower)</span>
                     </div>
                     <label className="switch">
                       <input 
                         type="checkbox" 
                         checked={config.deep_analysis}
                         onChange={(e) => updateConfig('deep_analysis', e.target.checked)}
                       />
                       <span className="slider round"></span>
                     </label>
                   </div>


                  <div className="mb-4">
                     <label>Custom Rules</label>
                     <textarea 
                        value={config.custom_rules} 
                        onChange={(e) => updateConfig('custom_rules', e.target.value)}
                        placeholder="e.g. Always include a quote..."
                        style={{ height: '80px', resize: 'none' }}
                     />
                  </div>
                  
                  <div className="mb-4">
                     <label>Export Template</label>
                     <textarea 
                        value={config.report_template} 
                        onChange={(e) => updateConfig('report_template', e.target.value)}
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
