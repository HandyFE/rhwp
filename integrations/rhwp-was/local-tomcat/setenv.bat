@echo off
rem rhwp.war 전용 Tomcat 인스턴스 (C:\Handyground\Workspace\FE-rhwp\tomcat)
set "RHWP_ROOT=%CATALINA_HOME%\.."
set "JAVA_OPTS=%JAVA_OPTS% -Dfile.encoding=UTF-8 -Xmx512m"
set "CATALINA_OPTS=%CATALINA_OPTS% -Drhwp.binary=%RHWP_ROOT%\rhwp-FE\target\pr-review\release\rhwp.exe"
set "CATALINA_OPTS=%CATALINA_OPTS% -Drhwp.workDir=%CATALINA_HOME%\rhwp-work"
set "CATALINA_OPTS=%CATALINA_OPTS% -Drhwp.workers=2 -Drhwp.timeoutMs=20000 -Drhwp.queueTimeoutMs=5000"
set "CATALINA_OPTS=%CATALINA_OPTS% -Drhwp.token=poc-token"
set "CATALINA_OPTS=%CATALINA_OPTS% -Drhwp.allowImageUrl=false"
