// A3SQL Patch Editor dialog — inherits from RscDisplayDefault
// Using safeZone coordinates for proper fullscreen rendering

#include "..\script_component.hpp"

class RscText_custom { type = 0; style = 2; font = "RobotoCondensed"; sizeEx = 0.03; colorText[] = {1,1,1,1}; colorBackground[] = {0,0,0,0}; text = ""; };
class RscEdit_custom: RscText_custom { type = 2; style = 16; colorBackground[] = {0.1,0.1,0.12,1}; colorText[] = {1,1,1,1}; };
class RscButton_custom: RscText_custom { type = 1; style = 2; colorBackground[] = {0.3,0.3,0.35,1}; colorText[] = {1,1,1,1}; };
class RscListBox_custom: RscText_custom { type = 5; style = 16; colorBackground[] = {0.05,0.05,0.07,1}; colorText[] = {1,1,1,1}; colorSelect[] = {0.3,0.3,0.4,1}; rowHeight = 0.03; };
class RscCombo_custom: RscText_custom { type = 4; style = 16; colorBackground[] = {0.1,0.1,0.12,1}; colorText[] = {1,1,1,1}; };
class RscCheckbox_custom: RscText_custom { type = 7; style = 0; colorBackground[] = {0.1,0.1,0.12,1}; colorText[] = {1,1,1,1}; };
class RscSlider_custom: RscText_custom { type = 8; style = 0; colorBackground[] = {0.3,0.3,0.35,1}; colorText[] = {1,1,1,1}; };

// Display base — minimal RscDisplayDefault stub
class RscDisplayDefault_custom {
    idd = -1;
    movingEnable = 1;
    enableDisplay = 1;
    enableSimulation = 1;
    class controls {};
    class controlsBackground {};
};

class a3sql_patch_editor: RscDisplayDefault_custom {
    idd = 12300;
    movingEnable = 1;
    enableSimulation = 0;
    enableDisplay = 1;
    onLoad = "uiNamespace setVariable ['a3sql_patch_editor', _this select 0]; call a3sql_patch_editor_fnc_gui_listRules;";

    class controlsBackground {
        class Background: RscText_custom {
            idc = -1;
            x = 0.15 * safezoneW + safezoneX;
            y = 0.1 * safezoneH + safezoneY;
            w = 0.7 * safezoneW;
            h = 0.8 * safezoneH;
            colorBackground[] = {0.1, 0.1, 0.12, 0.95};
        };
        class TitleBar: RscText_custom {
            idc = -1;
            text = "A3SQL Patch Editor";
            x = 0.15 * safezoneW + safezoneX;
            y = 0.1 * safezoneH + safezoneY;
            w = 0.7 * safezoneW;
            h = 0.05 * safezoneH;
            colorBackground[] = {0.15, 0.15, 0.18, 1};
            style = 2;
            sizeEx = 0.025;
            font = "RobotoCondensedBold";
        };
    };

    class controls {
        class RuleList: RscListBox_custom {
            idc = 100;
            x = 0.16 * safezoneW + safezoneX;
            y = 0.16 * safezoneH + safezoneY;
            w = 0.68 * safezoneW;
            h = 0.35 * safezoneH;
            onLBSelChanged = "_this call a3sql_patch_editor_fnc_gui_editRule";
        };
        class NameLabel: RscText_custom { idc = -1; text = "Name:"; x = 0.16 * safezoneW + safezoneX; y = 0.53 * safezoneH + safezoneY; w = 0.06 * safezoneW; h = 0.03 * safezoneH; };
        class NameEdit: RscEdit_custom { idc = 201; x = 0.22 * safezoneW + safezoneX; y = 0.53 * safezoneH + safezoneY; w = 0.15 * safezoneW; h = 0.03 * safezoneH; };
        class ActiveCheckbox: RscCheckbox_custom { idc = 202; x = 0.4 * safezoneW + safezoneX; y = 0.53 * safezoneH + safezoneY; w = 0.03 * safezoneW; h = 0.03 * safezoneH; };
        class PrioritySlider: RscSlider_custom { idc = 203; x = 0.5 * safezoneW + safezoneX; y = 0.53 * safezoneH + safezoneY; w = 0.3 * safezoneW; h = 0.03 * safezoneH; };
        class TargetTypeLabel: RscText_custom { idc = -1; text = "Type:"; x = 0.16 * safezoneW + safezoneX; y = 0.58 * safezoneH + safezoneY; w = 0.05 * safezoneW; h = 0.03 * safezoneH; };
        class TargetTypeCombo: RscCombo_custom { idc = 204; x = 0.21 * safezoneW + safezoneX; y = 0.58 * safezoneH + safezoneY; w = 0.12 * safezoneW; h = 0.03 * safezoneH; };
        class PropertyLabel: RscText_custom { idc = -1; text = "Property:"; x = 0.35 * safezoneW + safezoneX; y = 0.58 * safezoneH + safezoneY; w = 0.07 * safezoneW; h = 0.03 * safezoneH; };
        class PropertyEdit: RscEdit_custom { idc = 205; x = 0.42 * safezoneW + safezoneX; y = 0.58 * safezoneH + safezoneY; w = 0.35 * safezoneW; h = 0.03 * safezoneH; };
        class OperatorLabel: RscText_custom { idc = -1; text = "Op:"; x = 0.16 * safezoneW + safezoneX; y = 0.63 * safezoneH + safezoneY; w = 0.04 * safezoneW; h = 0.03 * safezoneH; };
        class OperatorCombo: RscCombo_custom { idc = 206; x = 0.2 * safezoneW + safezoneX; y = 0.63 * safezoneH + safezoneY; w = 0.12 * safezoneW; h = 0.03 * safezoneH; };
        class ValueLabel: RscText_custom { idc = -1; text = "Value:"; x = 0.35 * safezoneW + safezoneX; y = 0.63 * safezoneH + safezoneY; w = 0.05 * safezoneW; h = 0.03 * safezoneH; };
        class ValueEdit: RscEdit_custom { idc = 207; x = 0.4 * safezoneW + safezoneX; y = 0.63 * safezoneH + safezoneY; w = 0.37 * safezoneW; h = 0.03 * safezoneH; };
        class GroupLabel: RscText_custom { idc = -1; text = "Group:"; x = 0.16 * safezoneW + safezoneX; y = 0.68 * safezoneH + safezoneY; w = 0.05 * safezoneW; h = 0.03 * safezoneH; };
        class GroupEdit: RscEdit_custom { idc = 208; x = 0.21 * safezoneW + safezoneX; y = 0.68 * safezoneH + safezoneY; w = 0.15 * safezoneW; h = 0.03 * safezoneH; };
        class AddBtn: RscButton_custom { idc = 300; text = "Add"; x = 0.16 * safezoneW + safezoneX; y = 0.74 * safezoneH + safezoneY; w = 0.08 * safezoneW; h = 0.04 * safezoneH; action = "call a3sql_patch_editor_fnc_gui_addRule"; };
        class UpdateBtn: RscButton_custom { idc = 301; text = "Update"; x = 0.26 * safezoneW + safezoneX; y = 0.74 * safezoneH + safezoneY; w = 0.08 * safezoneW; h = 0.04 * safezoneH; action = "call a3sql_patch_editor_fnc_gui_addRule"; };
        class DeleteBtn: RscButton_custom { idc = 302; text = "Delete"; x = 0.36 * safezoneW + safezoneX; y = 0.74 * safezoneH + safezoneY; w = 0.08 * safezoneW; h = 0.04 * safezoneH; action = "call a3sql_patch_editor_fnc_gui_deleteRule"; };
        class RefreshBtn: RscButton_custom { idc = 303; text = "Refresh"; x = 0.46 * safezoneW + safezoneX; y = 0.74 * safezoneH + safezoneY; w = 0.08 * safezoneW; h = 0.04 * safezoneH; action = "call a3sql_patch_editor_fnc_gui_listRules"; };
        class SavePresetBtn: RscButton_custom { idc = 304; text = "Save Preset"; x = 0.56 * safezoneW + safezoneX; y = 0.74 * safezoneH + safezoneY; w = 0.12 * safezoneW; h = 0.04 * safezoneH; action = "call a3sql_patch_editor_fnc_gui_savePreset"; };
        class LoadPresetBtn: RscButton_custom { idc = 305; text = "Load Preset"; x = 0.7 * safezoneW + safezoneX; y = 0.74 * safezoneH + safezoneY; w = 0.12 * safezoneW; h = 0.04 * safezoneH; action = "call a3sql_patch_editor_fnc_gui_loadPreset"; };
    };
};
