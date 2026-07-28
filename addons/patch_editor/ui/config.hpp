// A3SQL Patch Editor dialog

class RscText_custom { type = 0; style = 2; font = "RobotoCondensed"; sizeEx = 0.03; colorText[] = {1,1,1,1}; colorBackground[] = {0,0,0,0}; text = ""; };
class RscEdit_custom: RscText_custom { type = 2; style = 16; colorBackground[] = {0.1,0.1,0.12,1}; colorText[] = {1,1,1,1}; };
class RscButton_custom: RscText_custom { type = 1; style = 2; colorBackground[] = {0.3,0.3,0.35,1}; colorText[] = {1,1,1,1}; };
class RscListBox_custom: RscText_custom { type = 5; style = 16; colorBackground[] = {0.05,0.05,0.07,1}; colorText[] = {1,1,1,1}; colorSelect[] = {0.3,0.3,0.4,1}; rowHeight = 0.03; };
class RscCombo_custom: RscText_custom { type = 4; style = 16; colorBackground[] = {0.1,0.1,0.12,1}; colorText[] = {1,1,1,1}; };
class RscCheckbox_custom: RscText_custom { type = 7; style = 0; colorBackground[] = {0.1,0.1,0.12,1}; colorText[] = {1,1,1,1}; };
class RscSlider_custom: RscText_custom { type = 8; style = 0; colorBackground[] = {0.3,0.3,0.35,1}; colorText[] = {1,1,1,1}; };

class a3sql_patch_editor {
    idd = 12300;
    movingEnable = 1;
    enableSimulation = 0;
    enableDisplay = 1;
    onLoad = "uiNamespace setVariable ['a3sql_patch_editor', _this select 0]; call a3sql_patch_editor_fnc_gui_listRules;";

    class controlsBackground {
        class Background: RscText_custom {
            idc = -1;
            x = 0.12; y = 0.1; w = 0.76; h = 0.82;
            colorBackground[] = {0.05, 0.05, 0.07, 0.95};
        };
        class TitleBar: RscText_custom {
            idc = -1;
            text = "A3SQL Patch Editor";
            x = 0.12; y = 0.1; w = 0.76; h = 0.04;
            colorBackground[] = {0.12, 0.12, 0.14, 1};
            style = 2; sizeEx = 0.025;
        };
    };

    class controls {
        class RuleList: RscListBox_custom {
            idc = 100;
            x = 0.14; y = 0.16; w = 0.72; h = 0.42;
            onLBSelChanged = "_this call a3sql_patch_editor_fnc_gui_editRule";
        };
        class NameEdit: RscEdit_custom { idc = 201; x = 0.2; y = 0.6; w = 0.18; h = 0.03; };
        class PrioritySlider: RscSlider_custom { idc = 203; x = 0.52; y = 0.6; w = 0.32; h = 0.03; };
        class TargetTypeCombo: RscCombo_custom { idc = 204; x = 0.2; y = 0.65; w = 0.14; h = 0.03; };
        class PropertyEdit: RscEdit_custom { idc = 205; x = 0.43; y = 0.65; w = 0.38; h = 0.03; };
        class OperatorCombo: RscCombo_custom { idc = 206; x = 0.2; y = 0.7; w = 0.14; h = 0.03; };
        class ValueEdit: RscEdit_custom { idc = 207; x = 0.43; y = 0.7; w = 0.38; h = 0.03; };
        class GroupEdit: RscEdit_custom { idc = 208; x = 0.2; y = 0.75; w = 0.18; h = 0.03; };
        class AddBtn: RscButton_custom { idc = 300; text = "Add"; x = 0.14; y = 0.81; w = 0.08; h = 0.04; action = "call a3sql_patch_editor_fnc_gui_addRule"; };
        class DeleteBtn: RscButton_custom { idc = 302; text = "Delete"; x = 0.24; y = 0.81; w = 0.08; h = 0.04; action = "call a3sql_patch_editor_fnc_gui_deleteRule"; };
        class RefreshBtn: RscButton_custom { idc = 303; text = "Refresh"; x = 0.34; y = 0.81; w = 0.08; h = 0.04; action = "call a3sql_patch_editor_fnc_gui_listRules"; };
        class SavePresetBtn: RscButton_custom { idc = 304; text = "Save Preset"; x = 0.44; y = 0.81; w = 0.1; h = 0.04; action = "call a3sql_patch_editor_fnc_gui_savePreset"; };
        class LoadPresetBtn: RscButton_custom { idc = 305; text = "Load Preset"; x = 0.56; y = 0.81; w = 0.1; h = 0.04; action = "call a3sql_patch_editor_fnc_gui_loadPreset"; };
    };
};
