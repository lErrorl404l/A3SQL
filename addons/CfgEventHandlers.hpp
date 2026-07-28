class Extended_PreInit_EventHandlers {
    class ADDON {
        init = "call compileScript ['z\a3sql\addons\ADDON_NAME\XEH_preInit.sqf']";
    snip };
snip };
snip class Extended_PostInit_EventHandlers {
    class ADDON {
        init = "call compileScript ['z\a3sql\addons\ADDON_NAME\XEH_postInit.sqf']";
    snip };
snip };
snip EOF
  sed -i "s/ADDON_NAME/$addon/g" "addons/$addon/CfgEventHandlers.hpp"
  echo "wrote $addon"
done
