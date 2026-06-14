import { useState, useEffect } from 'react';
import { AgentType, Session } from '../types/session';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { formatTimeAgo, truncatePath, statusConfig } from '@/lib/formatters';
import { openUrl } from '@tauri-apps/plugin-opener';
import { invoke } from '@tauri-apps/api/core';
import { Codex, Amp, Antigravity, OpenCode, Grok } from '@lobehub/icons';

// Agent type icons - official Claude icon from Bootstrap Icons, OpenCode pixelated "O" from logo
const ClaudeIcon = ({ className }: { className?: string }) => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" className={className || "w-4 h-4"}>
    <path d="m3.127 10.604 3.135-1.76.053-.153-.053-.085H6.11l-.525-.032-1.791-.048-1.554-.065-1.505-.08-.38-.081L0 7.832l.036-.234.32-.214.455.04 1.009.069 1.513.105 1.097.064 1.626.17h.259l.036-.105-.089-.065-.068-.064-1.566-1.062-1.695-1.121-.887-.646-.48-.327-.243-.306-.104-.67.435-.48.585.04.15.04.593.456 1.267.981 1.654 1.218.242.202.097-.068.012-.049-.109-.181-.9-1.626-.96-1.655-.428-.686-.113-.411a2 2 0 0 1-.068-.484l.496-.674L4.446 0l.662.089.279.242.411.94.666 1.48 1.033 2.014.302.597.162.553.06.17h.105v-.097l.085-1.134.157-1.392.154-1.792.052-.504.25-.605.497-.327.387.186.319.456-.045.294-.19 1.23-.37 1.93-.243 1.29h.142l.161-.16.654-.868 1.097-1.372.484-.545.565-.601.363-.287h.686l.505.751-.226.775-.707.895-.585.759-.839 1.13-.524.904.048.072.125-.012 1.897-.403 1.024-.186 1.223-.21.553.258.06.263-.218.536-1.307.323-1.533.307-2.284.54-.028.02.032.04 1.029.098.44.024h1.077l2.005.15.525.346.315.424-.053.323-.807.411-3.631-.863-.872-.218h-.12v.073l.726.71 1.331 1.202 1.667 1.55.084.383-.214.302-.226-.032-1.464-1.101-.565-.497-1.28-1.077h-.084v.113l.295.432 1.557 2.34.08.718-.112.234-.404.141-.444-.08-.911-1.28-.94-1.44-.759-1.291-.093.053-.448 4.821-.21.246-.484.186-.403-.307-.214-.496.214-.98.258-1.28.21-1.016.19-1.263.112-.42-.008-.028-.092.012-.953 1.307-1.448 1.957-1.146 1.227-.274.109-.477-.247.045-.44.266-.39 1.586-2.018.956-1.25.617-.723-.004-.105h-.036l-4.212 2.736-.75.096-.324-.302.04-.496.154-.162 1.267-.871z"/>
  </svg>
);

// Pi (Inflection AI) icon - from pi.dev/favicon.svg
const PiIcon = ({ className }: { className?: string }) => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 800" className={className || "w-4 h-4"}>
    <rect width="800" height="800" rx="120" fill="#09090b" />
    <path fill="#fff" fillRule="evenodd" d="M165.29 165.29H517.36V400H400v117.36H282.65v117.36H165.29Zm117.36-117.36V400H400V165.29Z" />
    <path fill="#fff" d="M517.36 400h117.36v234.72H517.36z" />
  </svg>
);

// Droid / Factory icon - from factory-zed-extension
const DroidIcon = ({ className }: { className?: string }) => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 900 900" fill="none" className={className || "w-4 h-4"}>
    <path fill="#fff" d="M622.037 192.524a10.58 10.58 0 0 1-4.056-2.001 10.573 10.573 0 0 1-3.98-7.89 10.595 10.595 0 0 1 .804-4.452c20.214-49.194 29.134-88.555 14.74-105.033-38.123-43.716-191.004 43.219-239.75 72.663a10.596 10.596 0 0 1-10.841.041 10.576 10.576 0 0 1-4.396-5.07c-20.491-49.089-42.031-83.233-63.865-84.714-57.871-3.96-104.516 165.624-118.17 220.895a10.574 10.574 0 0 1-1.993 4.057 10.572 10.572 0 0 1-7.884 3.977 10.586 10.586 0 0 1-4.447-.802c-49.194-20.215-88.572-29.134-105.033-14.74-43.717 38.123 43.202 191.005 72.644 239.75a10.553 10.553 0 0 1 1.454 4.279 10.573 10.573 0 0 1-6.481 10.958c-49.072 20.491-83.216 42.031-84.715 63.864-3.944 57.871 165.624 104.516 220.913 118.171a10.566 10.566 0 0 1 5.658 3.601 10.57 10.57 0 0 1 2.355 6.281 10.58 10.58 0 0 1-.799 4.442c-20.214 49.194-29.134 88.572-14.739 105.033 38.121 43.717 191.021-43.202 239.767-72.644a10.566 10.566 0 0 1 15.238 5.027c20.489 49.072 42.013 83.216 63.862 84.715 57.871 3.944 104.516-165.624 118.153-220.913a10.603 10.603 0 0 1 9.895-8.021 10.566 10.566 0 0 1 4.449.808c49.193 20.214 88.553 29.116 105.032 14.738 43.718-38.121-43.219-191.022-72.663-239.767a10.602 10.602 0 0 1 1.326-12.655 10.604 10.604 0 0 1 3.703-2.582c49.089-20.49 83.233-42.031 84.714-63.863 3.96-57.871-165.624-104.516-220.895-118.153Zm-66.394-55.478c11.123 19.938-46.196 152.797-88.83 245.724a8.36 8.36 0 0 1-8.239 4.855 8.374 8.374 0 0 1-7.412-6.043c-17.219-60.42-36.899-131.411-57.957-191.675a10.557 10.557 0 0 1 4.924-12.759c52.586-28.721 142.568-66.86 157.514-40.102ZM303.635 153.49c21.953 6.233 75.365 140.709 110.92 236.564a8.364 8.364 0 0 1-2.394 9.249 8.364 8.364 0 0 1-9.504.978c-54.943-30.493-119.013-66.824-176.522-94.546a10.565 10.565 0 0 1-5.528-12.501c16.926-57.44 53.532-148.095 83.028-139.744ZM137.064 343.322c19.921-11.123 152.795 46.197 245.707 88.83a8.369 8.369 0 0 1-1.189 15.652c-60.401 17.219-131.411 36.899-191.675 57.957a10.552 10.552 0 0 1-12.742-4.925c-28.668-52.584-66.876-142.568-40.101-157.514Zm16.443 252.009c6.217-21.953 140.709-75.365 236.564-110.921a8.368 8.368 0 0 1 10.227 11.898c-30.511 54.945-66.842 119.014-94.563 176.507a10.548 10.548 0 0 1-5.229 5.075 10.544 10.544 0 0 1-7.271.468c-57.441-16.822-148.095-53.531-139.728-83.027ZM343.34 761.902c-11.14-19.922 46.197-152.796 88.829-245.707a8.383 8.383 0 0 1 5.713-4.66 8.38 8.38 0 0 1 7.182 1.664 8.371 8.371 0 0 1 2.758 4.184c17.217 60.403 36.898 131.412 57.957 191.675a10.558 10.558 0 0 1-4.942 12.743c-52.568 28.668-142.568 66.875-157.445 40.101h-.052Zm252.009-16.443c-21.971-6.216-75.383-140.709-110.939-236.564a8.37 8.37 0 0 1 11.916-10.228c54.926 30.494 119.014 66.842 176.506 94.563a10.538 10.538 0 0 1 5.527 12.502c-16.909 57.526-53.515 148.094-83.01 139.727Zm166.57-189.834c-19.938 11.141-152.796-46.197-245.724-88.83a8.367 8.367 0 0 1-2.993-12.892 8.378 8.378 0 0 1 4.182-2.759c60.419-17.218 131.41-36.899 191.675-57.958a10.577 10.577 0 0 1 12.758 4.943c28.652 52.568 66.86 142.569 40.102 157.496Zm-16.444-252.009c-6.232 21.971-140.709 75.383-236.562 110.94a8.371 8.371 0 0 1-10.229-11.917c30.495-54.926 66.825-119.012 94.547-176.505a10.555 10.555 0 0 1 12.5-5.528c57.441 16.909 148.096 53.516 139.744 83.01Z" />
  </svg>
);

// Agent icon - each agent uses its own brand color
const AgentStatusIcon = ({ type, statusColor }: { type: AgentType, statusColor: string }) => {
  if (type === 'claude') {
    // Claude brand color: coral/orange #D77655
    return <ClaudeIcon className="w-4 h-4 fill-[#D77655]" />;
  }
  if (type === 'opencode') {
    // OpenCode: official monochrome logo, tinted via currentColor
    return <OpenCode size={16} className={`w-4 h-4 ${statusColor}`} />;
  }
  if (type === 'codex') {
    // OpenAI Codex: purple gradient from lobe-icons
    return <Codex.Color size={16} className="w-4 h-4" />;
  }
  if (type === 'amp') {
    // Amp: orange from lobe-icons
    return <Amp.Color size={16} className="w-4 h-4" />;
  }
  if (type === 'pi') {
    // Inflection Pi: black badge with white π (built into SVG)
    return <PiIcon className="w-4 h-4" />;
  }
  if (type === 'droid') {
    // Factory Droid: white logo, tinted via CSS
    return <DroidIcon className="w-4 h-4 fill-[#3B82F6]" />;
  }
  if (type === 'agy') {
    // Antigravity (Agy): from lobe-icons
    return <Antigravity.Color size={16} className="w-4 h-4" />;
  }
  if (type === 'grok') {
    // xAI Grok: official monochrome logo, tinted via currentColor
    return <Grok size={16} className={`w-4 h-4 ${statusColor}`} />;
  }
  return null;
};

interface SessionCardProps {
  session: Session;
  onClick: () => void;
}

// Helper to get/set custom data from localStorage
const CUSTOM_NAMES_KEY = 'agent-sessions-custom-names';
const CUSTOM_URLS_KEY = 'agent-sessions-custom-urls';

function getCustomNames(): Record<string, string> {
  try {
    const stored = localStorage.getItem(CUSTOM_NAMES_KEY);
    return stored ? JSON.parse(stored) : {};
  } catch {
    return {};
  }
}

function setCustomName(sessionId: string, name: string) {
  const names = getCustomNames();
  if (name.trim()) {
    names[sessionId] = name.trim();
  } else {
    delete names[sessionId];
  }
  localStorage.setItem(CUSTOM_NAMES_KEY, JSON.stringify(names));
}

function getCustomUrls(): Record<string, string> {
  try {
    const stored = localStorage.getItem(CUSTOM_URLS_KEY);
    return stored ? JSON.parse(stored) : {};
  } catch {
    return {};
  }
}

function setCustomUrl(sessionId: string, url: string) {
  const urls = getCustomUrls();
  if (url.trim()) {
    urls[sessionId] = url.trim();
  } else {
    delete urls[sessionId];
  }
  localStorage.setItem(CUSTOM_URLS_KEY, JSON.stringify(urls));
}

export function SessionCard({ session, onClick }: SessionCardProps) {
  const config = statusConfig[session.status];
  const [customName, setCustomNameState] = useState<string>('');
  const [customUrl, setCustomUrlState] = useState<string>('');
  const [isRenameOpen, setIsRenameOpen] = useState(false);
  const [isUrlOpen, setIsUrlOpen] = useState(false);
  const [renameValue, setRenameValue] = useState('');
  const [urlValue, setUrlValue] = useState('');

  // Load custom data on mount
  useEffect(() => {
    const names = getCustomNames();
    const urls = getCustomUrls();
    setCustomNameState(names[session.id] || '');
    setCustomUrlState(urls[session.id] || '');
  }, [session.id]);

  const displayName = customName || session.projectName;

  const handleRename = () => {
    setRenameValue(customName || session.projectName);
    setIsRenameOpen(true);
  };

  const handleSaveRename = () => {
    const newName = renameValue.trim();
    if (newName === session.projectName) {
      setCustomName(session.id, '');
      setCustomNameState('');
    } else {
      setCustomName(session.id, newName);
      setCustomNameState(newName);
    }
    setIsRenameOpen(false);
  };

  const handleResetName = () => {
    setCustomName(session.id, '');
    setCustomNameState('');
    setIsRenameOpen(false);
  };

  const handleSetUrl = () => {
    setUrlValue(customUrl);
    setIsUrlOpen(true);
  };

  const handleSaveUrl = () => {
    const newUrl = urlValue.trim();
    setCustomUrl(session.id, newUrl);
    setCustomUrlState(newUrl);
    setIsUrlOpen(false);
  };

  const handleClearUrl = () => {
    setCustomUrl(session.id, '');
    setCustomUrlState('');
    setIsUrlOpen(false);
  };

  const handleOpenUrl = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (customUrl) {
      // Add protocol if missing
      let url = customUrl;
      if (!url.startsWith('http://') && !url.startsWith('https://')) {
        url = 'http://' + url;
      }
      await openUrl(url);
    }
  };

  const handleOpenGitHub = async () => {
    if (session.githubUrl) {
      await openUrl(session.githubUrl);
    }
  };

  const handleKillSession = async () => {
    try {
      await invoke('kill_session', { pid: session.pid });
    } catch (error) {
      console.error('Failed to kill session:', error);
    }
  };

  return (
    <>
      <Card
        className={`relative group cursor-pointer transition-all duration-200 hover:shadow-lg py-0 gap-0 h-full flex flex-col ${config.cardBg} ${config.cardBorder} hover:border-primary/30`}
        onClick={onClick}
      >
        <CardContent className="p-4 flex flex-col flex-1">
          {/* Header: Project name + Menu + Status indicator */}
          <div className="flex items-start justify-between gap-2 mb-3">
            <div className="flex-1 min-w-0">
              <h3 className="font-semibold text-base text-foreground truncate group-hover:text-primary transition-colors">
                {displayName}
              </h3>
              <p className="text-xs text-muted-foreground truncate mt-0.5">
                {truncatePath(session.projectPath)}
              </p>
            </div>
            <div className="flex items-center gap-1.5 shrink-0">
              {/* URL Button - visible on hover if URL is set */}
              {customUrl && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-6 w-6 p-0 opacity-0 group-hover:opacity-100 transition-opacity hover:bg-primary/10"
                  onClick={handleOpenUrl}
                  title={customUrl}
                >
                  <svg
                    className="w-4 h-4 text-muted-foreground"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"
                    />
                  </svg>
                </Button>
              )}
              <DropdownMenu>
                <DropdownMenuTrigger asChild onClick={(e) => e.stopPropagation()}>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-6 w-6 p-0 opacity-0 group-hover:opacity-100 transition-opacity"
                  >
                    <svg
                      className="w-4 h-4 text-muted-foreground"
                      fill="currentColor"
                      viewBox="0 0 20 20"
                    >
                      <path d="M10 6a2 2 0 110-4 2 2 0 010 4zM10 12a2 2 0 110-4 2 2 0 010 4zM10 18a2 2 0 110-4 2 2 0 010 4z" />
                    </svg>
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" onClick={(e) => e.stopPropagation()}>
                  <DropdownMenuItem onClick={handleRename}>
                    <svg
                      className="w-4 h-4 mr-2"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
                      />
                    </svg>
                    Rename
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={handleSetUrl}>
                    <svg
                      className="w-4 h-4 mr-2"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1"
                      />
                    </svg>
                    {customUrl ? 'Edit URL' : 'Set URL'}
                  </DropdownMenuItem>
                  {session.githubUrl && (
                    <DropdownMenuItem onClick={handleOpenGitHub}>
                      <svg
                        className="w-4 h-4 mr-2"
                        fill="currentColor"
                        viewBox="0 0 24 24"
                      >
                        <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
                      </svg>
                      Open GitHub
                    </DropdownMenuItem>
                  )}
                  <DropdownMenuSeparator />
                  <DropdownMenuItem onClick={handleKillSession} className="text-destructive focus:text-destructive">
                    <svg
                      className="w-4 h-4 mr-2"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M6 18L18 6M6 6l12 12"
                      />
                    </svg>
                    Kill Session
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
              <AgentStatusIcon type={session.agentType} statusColor={config.fillColor} />
            </div>
          </div>

          {/* Git branch */}
          {session.gitBranch && (
            <div className="flex items-center gap-1.5 mb-3">
              <svg className="w-3.5 h-3.5 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 3v12M18 9a3 3 0 100-6 3 3 0 000 6zM6 21a3 3 0 100-6 3 3 0 000 6zM18 9a9 9 0 01-9 9" />
              </svg>
              <span className="text-xs text-muted-foreground truncate">
                {session.gitBranch}
              </span>
            </div>
          )}

          {/* Message Preview */}
          <div className="flex-1">
            {session.lastMessage && (
              <div className="text-sm text-muted-foreground line-clamp-2 leading-relaxed">
                {session.lastMessage}
              </div>
            )}
          </div>

          {/* Footer: Status Badge + Time */}
          <div className="flex items-center justify-between pt-3 mt-3 border-t border-border">
            <div className="flex items-center gap-2">
              <Badge variant="outline" className={config.badgeClassName}>
                {config.label}
              </Badge>
              {session.activeSubagentCount > 0 && (
                <span className="text-xs text-muted-foreground">
                  [+{session.activeSubagentCount}]
                </span>
              )}
            </div>
            <span className="text-xs text-muted-foreground">
              {formatTimeAgo(session.lastActivityAt)}
            </span>
          </div>
        </CardContent>
      </Card>

      {/* Rename Dialog */}
      <Dialog open={isRenameOpen} onOpenChange={setIsRenameOpen}>
        <DialogContent onClick={(e) => e.stopPropagation()}>
          <DialogHeader>
            <DialogTitle>Rename Session</DialogTitle>
          </DialogHeader>
          <div className="py-4">
            <Input
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              placeholder="Enter custom name"
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  handleSaveRename();
                }
              }}
              autoFocus
            />
            <p className="text-xs text-muted-foreground mt-2">
              Original: {session.projectName}
            </p>
          </div>
          <DialogFooter className="flex gap-2">
            {customName && (
              <Button variant="outline" onClick={handleResetName}>
                Reset to Original
              </Button>
            )}
            <Button variant="outline" onClick={() => setIsRenameOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleSaveRename}>Save</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* URL Dialog */}
      <Dialog open={isUrlOpen} onOpenChange={setIsUrlOpen}>
        <DialogContent onClick={(e) => e.stopPropagation()}>
          <DialogHeader>
            <DialogTitle>Set Development URL</DialogTitle>
          </DialogHeader>
          <div className="py-4">
            <Input
              value={urlValue}
              onChange={(e) => setUrlValue(e.target.value)}
              placeholder="e.g., localhost:3000"
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  handleSaveUrl();
                }
              }}
              autoFocus
            />
            <p className="text-xs text-muted-foreground mt-2">
              Quick access URL for this project (e.g., dev server)
            </p>
          </div>
          <DialogFooter className="flex gap-2">
            {customUrl && (
              <Button variant="outline" onClick={handleClearUrl}>
                Clear URL
              </Button>
            )}
            <Button variant="outline" onClick={() => setIsUrlOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleSaveUrl}>Save</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
