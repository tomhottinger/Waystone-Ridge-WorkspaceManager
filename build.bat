@echo off
cd /d C:\data\tom\home\proj\Waystone-Tools\Waystone-Ridge-WorkspaceManager
taskkill /im Waystone-Ridge.exe /f
del c:\data\tom\lib\Waystone-Tools\Waystone-Ridge\Waystone-Ridge.exe
cargo build --release
copy target\release\Waystone-Ridge.exe c:\data\tom\lib\Waystone-Tools\Waystone-Ridge\Waystone-Ridge.exe
start c:\data\tom\lib\Waystone-Tools\Waystone-Ridge\Waystone-Ridge.exe
pause


