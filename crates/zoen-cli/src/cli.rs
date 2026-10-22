//! Original argument parser/help; flags retain their existing behavior.
pub(crate) fn usage() -> ! {
    eprintln!(
        "usage: zoen [--home DIR] <command>\n\n\
         init --name NAME --handle HANDLE [--relay URL]\n\
         whoami | status | chats | verify\n\
         people QUERY\n\
         dm @HANDLE [TEXT]                end-to-end (MLS): the relay holds ciphertext\n\
         group TITLE @HANDLE... [--readable]   end-to-end unless --readable\n\
         encrypt CHAT                     make a relay-readable chat end-to-end, for good\n\
         add CHAT @HANDLE [--admin]       bring someone in (an admin can add and remove too)\n\
         remove CHAT @HANDLE              take someone out (end-to-end: they read nothing after)\n\
         keys CHAT                        an end-to-end chat's group: epoch, digest, members\n\
         send CHAT TEXT [--offline]      CHAT = @handle, title or space id\n\
         read CHAT [--older]              --older: ask your other device for an older page\n\
         agent-keys | agent-init --name N --handle H --owner ID --proof JSON --definition FILE\n\
         agent-host [--for SECS]          run this agent citizen (relay-backed cursors)\n\
         agent-inspect [--timeout MS]     read this agent's projected cursors/requests (no execution)\n\
         agent-enroll-request --owner ID --relay URL --name N --handle H --definition FILE\n\
         agent-enroll-wait [--timeout MS]  recover or wait for owner consent, then register\n\
         agent-enroll-cancel --offer-digest HASH   discard only an unused offer\n\
         app CHAT install APP JSON        install a catalog mini-app in CHAT\n\
         app CHAT view                    each mini-app in CHAT as you see it (JSON)\n\
         authorize-agent AGENT_ID         owner: sign an agent's root\n\
         agent-pause|agent-resume|agent-revoke @AGENT   owner controls\n\
         agent-define @AGENT --definition FILE [--version N]\n\
         link-request [--relay URL]       new device: show a code, become your account, get history\n\
         link CODE                        existing device: link the device showing CODE\n\
         history                          new device: finish (resume) the history transfer\n\
         devices | unlink DEVICE          your devices; unlink one (prefix ok)\n\
         invite CHAT\n\
         join CODE\n\
         agent CODE|HANDLE|LINK   open the chat behind an agent link\n\
         sync [--timeout MS]\n\
         open LINK                        (before init: where this install came from)\n\
         onboarding [--relay URL]         (the onboarding plan for that source and arm)\n\
         growth-sync [--health S C]       (config + source/exposures/health report)\n\
         variant FLAG\n\
         watch [--for SECONDS]\n\
         typing CHAT [--for SECONDS]\n\
         background CHAT --photo FILE     photo background for everyone (encrypted on the relay)\n\
         photo CHAT --out FILE            save the chat's background photo\n\
         profile set [--name N] [--bio B] [--photo FILE | --no-photo]\n\
         profile show [@HANDLE] [--out FILE]   what this device can read (FILE = their photo)\n\
         block @HANDLE | unblock @HANDLE\n\
         page import CHAT FILE... [--root DIR]  Markdown files as pages (path relative to DIR)\n\
         page export ITEM --out FILE | page export-all CHAT --dir DIR\n\
         page show ITEM | page check ITEM N | page add ITEM TEXT | page set ITEM N TEXT\n\
         page new CHAT TITLE | pages CHAT\n\
         file add CHAT FILE [--thumb PNG] [--path DIR] | files CHAT\n\
         file get ITEM --out FILE [--version N] [--thumb-out PNG]\n\
         file version ITEM FILE [--thumb PNG]\n\
         versions ITEM | restore ITEM N      ITEM = id (prefix ok), path or title\n\
         backup on (--password P | --recovery-key) | backup now | backup off | backup status\n\
         recover @HANDLE (--password P | --recovery-key KEY) [--relay URL]   a new device from a backup"
    );
    std::process::exit(2)
}

pub(crate) struct Cli {
    pub(crate) args: Vec<String>,
}

impl Cli {
    pub(crate) fn flag(&mut self, name: &str) -> Option<String> {
        let i = self.args.iter().position(|a| a == name)?;
        let v = self.args.get(i + 1).cloned();
        self.args.drain(i..(i + 2).min(self.args.len()));
        v
    }
    pub(crate) fn switch(&mut self, name: &str) -> bool {
        match self.args.iter().position(|a| a == name) {
            Some(i) => {
                self.args.remove(i);
                true
            }
            None => false,
        }
    }
}
